//! Native marks derive from actual effect/task filing and actual generation ends.
use super::*;
use crate::flow::templates::{CompiledInventory, TemplateCertificate};
use std::sync::OnceLock;

// wait_recorded is a blocking custody barrier. Full-recorder fixtures use an
// unrelated recoverable wait to return to their caller while retaining the
// original Decision/Cut clocks and exact pending author.
fn native_pressure<T>(
    fx: &mut PlaceFixture,
    advance: impl FnOnce(&mut PlaceFixture) -> Result<T, RunFailure>,
) -> Option<T> {
    let run = fx.run();
    let decision = run.context.deadlines.get(PumpDeadline::Decision);
    let cut = run.context.deadlines.get(PumpDeadline::Cut);
    let label = AbsoluteBound::after(run.clock.now().unwrap(), 1).unwrap();
    run.context
        .arm(PumpDeadline::Label, label, &mut run.clock)
        .unwrap();
    let result = advance(fx);
    let run = fx.run();
    assert_eq!(run.context.complete_wait(PumpDeadline::Label), Some(label));
    assert_eq!(run.context.deadlines.get(PumpDeadline::Decision), decision);
    assert_eq!(run.context.deadlines.get(PumpDeadline::Cut), cut);
    match result {
        Ok(progress) => Some(progress),
        Err(failure) => {
            assert_eq!(
                failure,
                RunFailure::Wait {
                    key: PumpDeadline::Label,
                    bound: label
                }
            );
            None
        }
    }
}

fn native_marks_pressure(fx: &mut PlaceFixture) -> Option<PlaceProgress> {
    native_pressure(fx, |fx| {
        let run = fx.run();
        run.context.advance_place_marks(&mut ports(&mut run.clock))
    })
}

fn native_reload_pressure(fx: &mut PlaceFixture) -> Option<ReloadProgress> {
    native_pressure(fx, |fx| {
        let run = &mut fx.summary.boot.member.run;
        run.context.advance_cut_and_reload(&mut PumpPorts {
            lifecycle: LifecyclePorts::default(),
            gate: None,
            decode: Some(&mut fx.decode),
            label: None,
            io: &mut run.clock,
        })
    })
}

fn active_snapshot(summary: &SummaryFixture) -> s::PlanSnapshot {
    let mut plan = summary.snapshot();
    plan.status = s::PlanStatus::Locked;
    plan.lock_source = Some(s::SourceBlockId {
        run: "prior-run".into(),
        sequence: 2.into(),
        block: 0.into(),
    });
    plan.steps = vec![s::PlanStep {
        step_id: s::StepId {
            seed_id: summary.seed().into(),
            local_id: 2.into(),
        },
        ordinal: 1.into(),
        instruction: s::StatementRef {
            block_id: s::SourceBlockId {
                run: "prior-run".into(),
                sequence: 1.into(),
                block: 0.into(),
            },
            binding: 0.into(),
        },
        category: "read".into(),
        group: None,
        status: s::StepStatus::Active,
        span: Some(s::SpanRef {
            seed_id: summary.seed().into(),
            local_id: 4.into(),
            revision: 1.into(),
        }),
        outcome_span: None,
        review_call: None,
        work: None,
        closure_source: None,
    }];
    plan
}
impl PlaceFixture {
    fn with_active_scope() -> (Self, s::PlanSnapshot) {
        let mut summary = SummaryFixture::new(vec![]);
        let plan = active_snapshot(&summary);
        let proof = summary.boot(Some(plan.clone()));
        let run = summary.run();
        run.context.install_place(proof, &mut run.clock).unwrap();
        let (decode, peer) = summary._decode.take().unwrap();
        (
            Self {
                summary,
                decode,
                peer,
                next_snapshot: 100,
                hold_files: false,
            },
            plan,
        )
    }
    fn closed_filing(&mut self, mut plan: s::PlanSnapshot) -> (FilingCompletion, s::PlanSnapshot) {
        let seed = self.summary.seed().to_owned();
        // Close includes StepOutcome, whose attributed turn is mandatory even
        // when its already-owed filing is completed at rest. No open turn bit
        // is manufactured by this scripted member/owner fixture.
        let turn = self
            .run()
            .context
            .open_turn
            .as_ref()
            .map(|turn| turn.0.clone())
            .unwrap_or_else(|| "owed-close-turn".into());
        let filing = self
            .summary
            .file(effect(&seed, "close", Some(&turn)), false);
        let position = committed_position(&filing);
        plan.last_source = position.clone();
        plan.status = s::PlanStatus::Complete;
        for step in &mut plan.steps {
            step.status = s::StepStatus::Closed;
            step.closure_source = Some(position.block_id.clone());
            step.outcome_span = Some(s::SpanRef {
                seed_id: seed.clone(),
                local_id: 5.into(),
                revision: 1.into(),
            });
        }
        (filing, plan)
    }
    fn closed_effect(&mut self, plan: s::PlanSnapshot) -> EffectPlanSummary {
        let (filing, plan) = self.closed_filing(plan);
        self.summary.begin_effect(filing);
        self.summary.release(Some(plan));
        let PlanSummaryCompletion::Effect(proof) = self.summary.finish() else {
            panic!("real close refresh failed");
        };
        proof
    }
    fn finish_native(&mut self) -> NativeMarkCompletion {
        for _ in 0..128 {
            let run = self.run();
            run.context
                .advance_place_marks(&mut ports(&mut run.clock))
                .unwrap();
            if let Some(done) = run.context.take_place_marks(&mut run.clock).unwrap() {
                return done;
            }
        }
        panic!("native positions did not pass their real recorder boundaries");
    }
}

#[test]
fn place_native_close_uses_last_completed_generation_and_never_outcome_statement_end() {
    let (mut fx, plan) = PlaceFixture::with_active_scope();
    let units = fx.text(8, "record");
    fx.turn("scope-work");
    let generation = fx.generation(units, 3, 1, "Read result.", Finish::Completed);
    let assistant_id = generation.assistant_id();
    let before = fx.journal();
    let prior_marks = fx.records(Kind::StateMarkPosition).len();
    let proof = fx.closed_effect(plan);
    let source = committed_position(proof.filing()).block_id;
    let run = fx.run();
    assert!(
        !run.context.native_place_cache_ready(),
        "a refreshed cache does not replace the closing receipt handoff"
    );
    run.context
        .begin_place_effect_marks(proof, &mut run.clock)
        .unwrap();
    assert!(receive(fx.peer.as_fd()).is_none());
    let done = fx.finish_native();
    assert_eq!(
        done.positions().len(),
        2,
        "observed task and step close; step_outcome stays placed-only"
    );
    for mark in done.positions() {
        assert!(matches!(mark.span.local_id.0, 1 | 4));
        assert_eq!(
            mark.end,
            t::ArgumentCursor::Message {
                message_id: assistant_id.0.into(),
                block_end: 1.into(),
                byte_end: None
            }
        );
        assert_eq!(
            mark.source,
            t::MarkPositionSource::Native {
                event: t::SourceBlockId {
                    run: source.run.clone(),
                    sequence: source.sequence.0.into(),
                    block: source.block.0.into()
                }
            }
        );
    }
    assert_eq!(fx.journal().1, before.1);
    assert_eq!(fx.journal().6, before.6);
    assert_eq!(fx.records(Kind::StateMarkPosition).len(), prior_marks + 2);
    assert_eq!(done.prerequisite().filing().sequence().0, source.sequence.0);
    assert!(fx.run().context.native_place_cache_ready());
    assert!(fx.run().context.completed_author.is_none());
    let units = fx.text(9, "next");
    fx.generation(units, 2, 0, "", Finish::Completed);
}

#[test]
fn place_unobserved_scopes_get_no_native_position_from_filed_close_alone() {
    let (mut fx, plan) = PlaceFixture::with_active_scope();
    fx.turn("scope-work");
    let before = fx.records(Kind::StateMarkPosition).len();
    let proof = fx.closed_effect(plan);
    let run = fx.run();
    run.context
        .begin_place_effect_marks(proof, &mut run.clock)
        .unwrap();
    assert!(fx.finish_native().positions().is_empty());
    assert_eq!(fx.records(Kind::StateMarkPosition).len(), before);
    assert_eq!(fx.journal().6, 0);
}

#[test]
fn place_changed_summary_blocks_actual_next_measure_until_native_handoff_completes() {
    let (mut fx, plan) = PlaceFixture::with_active_scope();
    let units = fx.text(8, "record");
    let later = fx.text(9, "later");
    fx.turn("scope-work");
    fx.generation(units, 3, 0, "", Finish::Completed);
    let proof = fx.closed_effect(plan);
    fx.decision();
    let request = fx.request(later);
    let run = &mut fx.summary.boot.member.run;
    let error = run
        .context
        .begin_place(request, &mut fx.decode, &mut run.clock)
        .unwrap_err();
    assert_eq!(error.refusal, PlaceError::WrongProof);
    assert!(error.failure.is_none());
    assert!(receive(fx.peer.as_fd()).is_none());
    let run = fx.run();
    run.context
        .begin_place_effect_marks(proof, &mut run.clock)
        .unwrap();
    fx.finish_native();
    // Native positions advance revision; the old proposal must be prepared
    // again using its retained material, without accepting the old projection.
    fx.generation(error.input.selection, 2, 0, "", Finish::Completed);
}

#[test]
fn place_native_effect_rejects_a_foreign_real_receipt_even_with_matching_read_values() {
    let (mut first, plan) = PlaceFixture::with_active_scope();
    first.turn("scope-work");
    let foreign = first.closed_effect(plan);
    let (mut second, plan) = PlaceFixture::with_active_scope();
    second.turn("scope-work");
    let own = second.closed_effect(plan);
    assert_eq!(foreign.summary(), own.summary());
    let run = second.run();
    let error = run
        .context
        .begin_place_effect_marks(foreign, &mut run.clock)
        .unwrap_err();
    assert_eq!(error.refusal, PlaceError::WrongProof);
    assert!(error.failure.is_none());
    run.context
        .begin_place_effect_marks(own, &mut run.clock)
        .unwrap();
    second.finish_native();
    let run = first.run();
    run.context
        .begin_place_effect_marks(error.prerequisite, &mut run.clock)
        .unwrap();
    first.finish_native();
}

#[test]
fn place_native_position_pressure_keeps_exact_receipt_and_original_wait() {
    let (mut fx, plan) = PlaceFixture::with_active_scope();
    let units = fx.text(8, "record");
    fx.turn("scope-work");
    fx.generation(units, 3, 0, "", Finish::Completed);
    let proof = fx.closed_effect(plan);
    let original = proof.filing().wait_bound();
    let run = fx.run();
    run.context
        .begin_place_effect_marks(proof, &mut run.clock)
        .unwrap();
    fx.summary.boot.member.fill_trace();
    let mut ticket = None;
    for _ in 0..128 {
        native_marks_pressure(&mut fx);
        if let Some(author) = fx.run().context.pending_author() {
            assert_eq!(author.event.kind, Kind::StateMarkPosition);
            ticket = Some(author.ticket);
            break;
        }
    }
    let ticket = ticket.expect("actual pending native mark author");
    for _ in 0..8 {
        assert_ne!(
            native_marks_pressure(&mut fx),
            Some(PlaceProgress::Complete)
        );
        let run = fx.run();
        assert_eq!(run.context.pending_author().unwrap().ticket, ticket);
        assert_eq!(
            run.context.deadlines.get(PumpDeadline::Decision),
            Some(original)
        );
        assert!(run.context.complete_wait(PumpDeadline::Decision).is_none());
        assert!(!run.context.native_place_cache_ready());
    }
    fx.summary.drain();
    assert_eq!(fx.finish_native().positions().len(), 2);
    assert!(fx.run().context.completed_author.is_none());
}

fn task_inventory() -> CompiledInventory {
    static INVENTORY: OnceLock<Vec<s::TemplateSpec>> = OnceLock::new();
    CompiledInventory::new(INVENTORY.get_or_init(|| {
        vec![s::TemplateSpec {
            template_id: "mark.begin_task".into(),
            kind: "state.mark".into(),
            parts: vec![s::TemplatePart::Literal("Read task.".into())],
        }]
    }))
    .unwrap()
}
fn task_place_fixture() -> (PlaceFixture, TemplateCertificate) {
    let mut input =
        crate::flow::context::input_tests::InputFixture::with_template_inventory(&task_inventory());
    input
        .run
        .context
        .begin_template_checks(task_inventory(), &mut input.run.clock)
        .unwrap();
    for _ in 0..128 {
        let progress = input
            .run
            .context
            .advance_template_checks(&mut input.run.clock)
            .unwrap();
        input.classifier_transport();
        if let Receive::Complete(message) = input.peer.try_receive().unwrap() {
            let request: s::Ste100Request =
                s::decode_strict(std::str::from_utf8(&message.payload).unwrap()).unwrap();
            input.reply(&s::Ste100Reply {
                request_id: request.request_id,
                target: request.target,
                scheme_id: request.scheme_id,
                result: s::Ste100Result::Accepted { bindings: vec![] },
            });
        }
        if progress == TemplateProgress::Complete {
            break;
        }
    }
    let certificate = input
        .run
        .context
        .take_template_certificate(&mut input.run.clock)
        .unwrap()
        .unwrap();
    let mut boot = BootFixture::from_templates(TemplateFixture { input });
    let entry = boot.entry();
    let request = boot.begin();
    boot.released(&request, entry);
    assert!(matches!(boot.finish(), BootReadCompletion::Seated));
    let (decode, peer) = DecodeChannel::pair().unwrap();
    let mut summary = SummaryFixture::from_open(
        OpenFixture {
            boot,
            decode: DecodePort::from_channel(decode).unwrap(),
            peer,
        },
        vec![],
    );
    let proof = summary.boot(None);
    let run = summary.run();
    run.context.install_place(proof, &mut run.clock).unwrap();
    let (decode, peer) = summary._decode.take().unwrap();
    (
        PlaceFixture {
            summary,
            decode,
            peer,
            next_snapshot: 100,
            hold_files: false,
        },
        certificate,
    )
}
fn task_member(fx: &mut PlaceFixture, next_span: u64) {
    fx.summary.transport(TransportSource::State);
    let mut bytes = [0; 65536];
    let n = match fx.summary.boot.member.run.state.read(&mut bytes) {
        Ok(0) => panic!("task member closed"),
        Ok(n) => n,
        Err(e) if e.kind() == io::ErrorKind::WouldBlock => return,
        Err(e) => panic!("task member read: {e}"),
    };
    let request = s::decode_state_line(&bytes[..n], s::FrameDirection::Request).unwrap();
    match &request {
        s::StateFrame::Control { frame, .. } => {
            let answer = match frame {
                s::StateControl::Read {
                    selector: s::ReadSelector::Marks {},
                } => {
                    let raw = serde_json::to_vec(&s::QueryResult::Marks {
                        next_span_id: next_span.into(),
                        current_task: None,
                        open_scopes: vec![],
                    })
                    .unwrap();
                    page(777, 0, raw.len(), &raw)
                }
                s::StateControl::Release { snapshot_id } => s::StateControlAnswer::Released {
                    snapshot_id: *snapshot_id,
                },
                _ => panic!("unexpected task read"),
            };
            fx.summary.boot.member.respond(&request, answer);
        }
        s::StateFrame::File { filing, .. } => {
            fx.summary.files.push(filing.clone());
            let began = fx.summary.boot.member.now() + 1;
            let observation = operation(&request, fx.summary.boot.member.next_operation, began);
            fx.summary.boot.member.next_operation += 1;
            fx.summary
                .boot
                .member
                .receive(&s::StateFrame::Filed {
                    common: request.common().clone(),
                    block_id: filing.block_id.clone(),
                    observation,
                })
                .unwrap();
        }
        _ => panic!("unexpected task member request"),
    }
}
fn actual_task(fx: &mut PlaceFixture, certificate: &TemplateCertificate) -> BegunTask {
    actual_task_at(fx, certificate, 1)
}
fn actual_task_at(
    fx: &mut PlaceFixture,
    certificate: &TemplateCertificate,
    next_span: u64,
) -> BegunTask {
    fx.decision();
    let run = fx.run();
    run.context
        .begin_input(
            weaver_types::TurnFrame::carry_from(br#"{"text":"Read prompt."}"#, 42),
            &mut run.clock,
        )
        .unwrap();
    let mut token = None;
    for _ in 0..128 {
        let run = fx.run();
        run.context
            .advance_input(&mut ports(&mut run.clock))
            .unwrap();
        fx.classifier(false);
        let run = fx.run();
        token = run.context.take_accepted_input(&mut run.clock).unwrap();
        if token.is_some() {
            break;
        }
    }
    let run = fx.run();
    run.context
        .begin_task(
            token.expect("real accepted input"),
            weaver_trace::TurnRef(format!("task-turn-{next_span}")),
            certificate,
            &mut run.clock,
        )
        .unwrap();
    for _ in 0..256 {
        let run = fx.run();
        run.context
            .advance_task(&mut ports(&mut run.clock))
            .unwrap();
        fx.classifier(false);
        task_member(fx, next_span);
        let run = fx.run();
        if let Some(done) = run.context.take_task(&mut run.clock).unwrap() {
            let TaskCompletion::Begun(task) = done else {
                panic!("real task refused");
            };
            return task;
        }
    }
    panic!("task owner failed to produce its real receipt chain");
}

#[test]
fn place_native_prompt_requires_real_begin_task_receipt_and_complete_argument_interval() {
    let (mut fx, certificate) = task_place_fixture();
    let task = actual_task(&mut fx, &certificate);
    let scope = task.scope().clone();
    let source = task.input().source().clone();
    let event = task.mark().unwrap().sequence();
    assert_eq!(
        fx.records(Kind::StateMarkPosition).len(),
        1,
        "Filed alone is not a resident prompt"
    );
    let run = fx.run();
    let units = run
        .context
        .hold_task_material(task, &mut run.clock)
        .unwrap()
        .into_units();
    let before = fx.journal();
    let grant = fx.grant(units, 3);
    assert_eq!(
        fx.records(Kind::StateMarkPosition).len(),
        1,
        "Recorded proposal is not a native occurrence"
    );
    fx.start_generation(grant);
    fx.sent_generation();
    fx.answer(&TokenAnswer::Generated(generated(
        before.1,
        3,
        0,
        "",
        Finish::Completed,
        before.2,
    )));
    assert!(matches!(
        fx.finish_generation(),
        GenerationCompletion::Generated(_)
    ));
    let record = fx.records(Kind::StateMarkPosition).pop().unwrap();
    assert_eq!(
        record["payload"]["span"],
        serde_json::to_value(scope.prompt).unwrap()
    );
    assert_eq!(
        record["payload"]["source"]["native"]["event"]["sequence"],
        event.0.to_string()
    );
    let append = fx.records(Kind::StatePlacement).pop().unwrap();
    assert_eq!(
        record["payload"]["end"]["message"]["message_id"],
        append["payload"]["message_ids"][0]
    );
    let run = fx.run();
    let prefix = run
        .context
        .prepare_prefix(None, s::CutMethod::Flush, &mut run.clock)
        .unwrap();
    assert!(prefix.coverage().iter().any(|material| matches!(material, s::MaterialRef::Text {
        source: s::ContentRef::Event { block_id }, span } if block_id == &source && span.start.0 == 0 && span.end.0 == 12)));
}

const _: fn() = || {
    trait PrivateNative<M> {
        fn pin() {}
    }
    impl<T: ?Sized> PrivateNative<()> for T {}
    impl<T: Clone> PrivateNative<u8> for T {}
    impl<T: Default> PrivateNative<u16> for T {}
    impl<T: From<s::MarkPosition>> PrivateNative<u32> for T {}
    impl<T: From<s::ArgumentCursor>> PrivateNative<u64> for T {}
    let _ = <NativePosition as PrivateNative<_>>::pin;
    let _ = <NativeMarkCompletion as PrivateNative<_>>::pin;
};

impl PlaceFixture {
    fn from_plan(plan: impl FnOnce(&SummaryFixture) -> s::PlanSnapshot) -> Self {
        let mut summary = SummaryFixture::new(vec![]);
        let snapshot = plan(&summary);
        let proof = summary.boot(Some(snapshot));
        let run = summary.run();
        run.context.install_place(proof, &mut run.clock).unwrap();
        let (decode, peer) = summary._decode.take().unwrap();
        Self {
            summary,
            decode,
            peer,
            next_snapshot: 100,
            hold_files: false,
        }
    }
    fn refresh_native(
        &mut self,
        event: AuthoredEvent,
        mut snapshot: s::PlanSnapshot,
    ) -> EffectPlanSummary {
        let filing = self.summary.file(event, false);
        snapshot.last_source = committed_position(&filing);
        self.summary.begin_effect(filing);
        self.summary.release(Some(snapshot));
        let PlanSummaryCompletion::Effect(proof) = self.summary.finish() else {
            panic!("real native refresh refused");
        };
        proof
    }
}

#[test]
fn place_native_operator_prompt_registers_from_existing_full_resident_text_at_effect_receipt() {
    for operation in ["authorize", "lock"] {
        let mut fx = PlaceFixture::new();
        let mut entry = text_entry(1, "prompt");
        let s::ReadSource::Event(source) = &mut entry.source else {
            unreachable!()
        };
        source.block_id.run = "prior-run".into();
        source.envelope.run = "prior-run".into();
        let units = fx.material(
            s::ReadSelector::History {
                noun: "prompt".into(),
                verb: None,
                condition: None,
            },
            vec![entry],
        );
        fx.turn("operator-prompt");
        let completed = fx.generation(units, 3, 0, "", Finish::Completed);
        let prior = fx.records(Kind::StatePlacement).pop().unwrap();
        let mut snapshot = fx.summary.snapshot();
        if operation == "lock" {
            snapshot.status = s::PlanStatus::Locked;
            snapshot.lock_source = Some(snapshot.operator_source.clone());
        }
        let event = effect(fx.summary.seed(), operation, None);
        let proof = fx.refresh_native(event, snapshot.clone());
        let source = committed_position(proof.filing());
        let run = fx.run();
        run.context
            .begin_place_effect_marks(proof, &mut run.clock)
            .unwrap();
        let done = fx.finish_native();
        assert_eq!(
            done.positions().len(),
            1,
            "already resident prompt needs no additional generation"
        );
        let position = &done.positions()[0];
        assert_eq!(
            position.span,
            crate::flow::mirror::span_ref(&snapshot.task.prompt)
        );
        assert_eq!(
            position.end,
            t::ArgumentCursor::Message {
                message_id: serde_json::from_value::<weaver_types::state_flow::DecimalU64>(
                    prior["payload"]["message_ids"][0].clone(),
                )
                .unwrap()
                .0
                .into(),
                block_end: 1.into(),
                byte_end: None
            }
        );
        assert_ne!(
            position.end,
            t::ArgumentCursor::Message {
                message_id: completed.assistant_id().0.into(),
                block_end: 0.into(),
                byte_end: None
            }
        );
        assert_eq!(
            position.source,
            t::MarkPositionSource::Native {
                event: crate::flow::mirror::source(&source.block_id)
            }
        );
        assert_eq!(done.recorded_sequences().len(), 1);
        assert!(receive(fx.peer.as_fd()).is_none());
    }
}

#[test]
fn place_native_group_and_step_open_only_from_actual_activation_then_close_at_last_generation() {
    let mut snapshot = None;
    let mut fx = PlaceFixture::from_plan(|summary| {
        let mut plan = active_snapshot(summary);
        plan.steps[0].status = s::StepStatus::Waiting;
        plan.steps[0].span = None;
        plan.steps[0].group = Some("group".into());
        plan.groups = vec![s::PlanGroup {
            id: "group".into(),
            step_ids: vec![plan.steps[0].step_id.clone()],
            span: None,
        }];
        snapshot = Some(plan.clone());
        plan
    });
    let mut plan = snapshot.unwrap();
    let group = s::SpanRef {
        seed_id: fx.summary.seed().into(),
        local_id: 6.into(),
        revision: 1.into(),
    };
    let step = s::SpanRef {
        seed_id: fx.summary.seed().into(),
        local_id: 4.into(),
        revision: 1.into(),
    };
    plan.steps[0].status = s::StepStatus::Active;
    plan.steps[0].span = Some(step);
    plan.groups[0].span = Some(group.clone());
    fx.turn("scope-work");
    let mut event = effect(fx.summary.seed(), "activate", Some("scope-work"));
    let Some(Payload::PlanEffect(record)) = &mut event.payload else {
        unreachable!()
    };
    let t::PlanEffect::Activate { group_span, .. } = &mut record.context else {
        unreachable!()
    };
    *group_span = Some(crate::flow::mirror::span_ref(&group));
    let proof = fx.refresh_native(event, plan.clone());
    let run = fx.run();
    run.context
        .begin_place_effect_marks(proof, &mut run.clock)
        .unwrap();
    assert!(
        fx.finish_native().positions().is_empty(),
        "activation is not an observed endpoint"
    );
    let units = fx.text(8, "record");
    let generated = fx.generation(units, 3, 0, "", Finish::Completed);
    let proof = fx.closed_effect(plan);
    let run = fx.run();
    run.context
        .begin_place_effect_marks(proof, &mut run.clock)
        .unwrap();
    let done = fx.finish_native();
    let mut ids: Vec<_> = done.positions().iter().map(|p| p.span.local_id.0).collect();
    ids.sort();
    assert_eq!(ids, [1, 4, 6]);
    assert_eq!(done.recorded_sequences().len(), 3);
    for mark in done.positions() {
        assert_eq!(
            mark.end,
            t::ArgumentCursor::Message {
                message_id: generated.assistant_id().0.into(),
                block_end: 0.into(),
                byte_end: None
            }
        );
    }
}

#[test]
fn place_native_replacement_retires_prior_observed_scopes_without_reopening_old_prompt() {
    let (mut fx, plan) = PlaceFixture::with_active_scope();
    let units = fx.text(8, "old");
    fx.turn("replace-scope");
    let generated = fx.generation(units, 3, 0, "", Finish::Completed);
    let mut replacement = plan.clone();
    replacement.plan.revision = 2.into();
    replacement.status = s::PlanStatus::Authorized;
    replacement.lock_source = None;
    replacement.steps.clear();
    replacement.groups.clear();
    // Task spans are immutable revision 1 identities; replacement allocates
    // new local IDs while advancing only the plan revision.
    replacement.task.task.local_id = 10.into();
    replacement.task.prompt.local_id = 11.into();
    let mut event = effect(fx.summary.seed(), "replace", None);
    let Some(Payload::PlanEffect(record)) = &mut event.payload else {
        unreachable!()
    };
    let t::PlanEffect::Replace { task, .. } = &mut record.context else {
        unreachable!()
    };
    *task = t::TaskScopeRef {
        task: crate::flow::mirror::span_ref(&replacement.task.task),
        prompt: crate::flow::mirror::span_ref(&replacement.task.prompt),
    };
    let proof = fx.refresh_native(event, replacement.clone());
    let run = fx.run();
    run.context
        .begin_place_effect_marks(proof, &mut run.clock)
        .unwrap();
    let done = fx.finish_native();
    assert_eq!(done.positions().len(), 2);
    for mark in done.positions() {
        assert!(matches!(mark.span.local_id.0, 1 | 4));
        assert_eq!(mark.span.revision.0, 1);
        assert_eq!(
            mark.end,
            t::ArgumentCursor::Message {
                message_id: generated.assistant_id().0.into(),
                block_end: 0.into(),
                byte_end: None
            }
        );
    }
    let units = fx.text(9, "new");
    fx.generation(units, 2, 0, "", Finish::Completed);
    assert_eq!(
        fx.records(Kind::StateMarkPosition).len(),
        3,
        "no retained prompt Text means no invented native prompt"
    );
}

#[test]
fn place_native_cancelled_close_retains_actual_positions_receipt_and_recorded_sequences() {
    let (mut fx, plan) = PlaceFixture::with_active_scope();
    let units = fx.text(8, "record");
    fx.turn("scope-work");
    fx.generation(units, 3, 0, "", Finish::Completed);
    let proof = fx.closed_effect(plan);
    let original = proof.filing().wait_bound();
    let sequence = proof.filing().sequence();
    let run = fx.run();
    run.context
        .begin_place_effect_marks(proof, &mut run.clock)
        .unwrap();
    // Record one position before revocation; the later account must retain it.
    for _ in 0..128 {
        let run = fx.run();
        run.context
            .advance_place_marks(&mut ports(&mut run.clock))
            .unwrap();
        if fx.records(Kind::StateMarkPosition).len() == 2 {
            break;
        }
    }
    assert_eq!(fx.records(Kind::StateMarkPosition).len(), 2);
    let run = fx.run();
    run.context.cancel_place_marks().unwrap();
    let leave = AbsoluteBound::after(run.clock.now().unwrap(), PLACE_WAIT / 2).unwrap();
    run.context
        .begin_place_release(leave, &mut run.clock)
        .unwrap();
    let mut account = None;
    for _ in 0..128 {
        let run = &mut fx.summary.boot.member.run;
        run.context
            .advance_place_account(&mut PumpPorts {
                lifecycle: LifecyclePorts::default(),
                gate: None,
                decode: Some(&mut fx.decode),
                label: None,
                io: &mut run.clock,
            })
            .unwrap();
        account = run.context.take_place_account();
        if account.is_some() {
            break;
        }
    }
    let account = account.expect("cancelled native account must settle");
    assert!(account.incomplete().is_none());
    assert_eq!(
        account.native_prerequisite().unwrap().filing().sequence(),
        sequence
    );
    assert_eq!(
        account.native_prerequisite().unwrap().filing().wait_bound(),
        original
    );
    assert_eq!(account.native_positions().len(), 2);
    assert_eq!(account.native_recorded_sequences().len(), 2);
    assert_eq!(fx.records(Kind::StateMarkPosition).len(), 3);
    assert!(!fx.run().context.native_place_cache_ready());
    assert!(fx.run().context.failure().is_none());
    assert!(fx.run().context.completed_author.is_none());
    assert!(receive(fx.peer.as_fd()).is_none());
}

#[test]
fn place_native_prompt_removed_by_cut_can_register_a_new_actual_interval() {
    let mut fx = PlaceFixture::new();
    let prompt = |fx: &mut PlaceFixture| {
        let mut entry = text_entry(1, "prompt");
        let s::ReadSource::Event(source) = &mut entry.source else {
            unreachable!()
        };
        source.block_id.run = "prior-run".into();
        source.envelope.run = "prior-run".into();
        fx.material(
            s::ReadSelector::History {
                noun: "prompt".into(),
                verb: None,
                condition: None,
            },
            vec![entry],
        )
    };
    let units = prompt(&mut fx);
    fx.turn("old-prompt");
    fx.generation(units, 3, 0, "", Finish::Completed);
    let snapshot = fx.summary.snapshot();
    let event = effect(fx.summary.seed(), "authorize", None);
    let proof = fx.refresh_native(event, snapshot.clone());
    let run = fx.run();
    run.context
        .begin_place_effect_marks(proof, &mut run.clock)
        .unwrap();
    let first = fx.finish_native().positions()[0].clone();
    let reminder = prompt(&mut fx);
    fx.close_turn();
    let boot = fx
        .records(Kind::StateMarkPosition)
        .into_iter()
        .next()
        .unwrap();
    let cut = s::CutReference {
        span: serde_json::from_value(boot["payload"]["span"].clone()).unwrap(),
        occurrence_id: serde_json::from_value(boot["payload"]["id"].clone()).unwrap(),
    };
    let prefix = fx.prefix(Some(cut), s::CutMethod::Flush);
    let staged = fx.staged(prefix, ReloadTail::held(reminder), 3);
    assert!(
        !fx.journal()
            .7
            .iter()
            .any(|(ordinal, _, _)| *ordinal == first.id.ordinal.0)
    );
    fx.turn("new-prompt");
    fx.begin_staged(staged, vec![]);
    let TokenDirective::MeasureDelta { keep, .. } = fx.next_measure() else {
        unreachable!()
    };
    fx.answer(&fx.measured(keep, 3));
    let PlacementCompletion::Granted(grant) = fx.take() else {
        panic!("valid prompt carry refused");
    };
    fx.start_generation(grant);
    fx.sent_generation();
    fx.answer(&TokenAnswer::Generated(generated(
        keep,
        3,
        0,
        "",
        Finish::Completed,
        fx.journal().2,
    )));
    assert!(matches!(
        fx.finish_generation(),
        GenerationCompletion::Generated(_)
    ));
    let later = fx.records(Kind::StateMarkPosition).pop().unwrap();
    assert_eq!(
        later["payload"]["span"],
        serde_json::to_value(snapshot.task.prompt).unwrap()
    );
    assert_ne!(
        later["payload"]["id"]["ordinal"],
        first.id.ordinal.0.to_string()
    );
    assert_ne!(
        later["payload"]["end"],
        serde_json::to_value(first.end).unwrap()
    );
    assert!(later["payload"]["source"].get("native").is_some());
}

#[test]
fn place_native_prompt_requires_whole_text_not_an_equal_span_name_or_partial_interval() {
    let mut fx = PlaceFixture::new();
    let snapshot = fx.summary.snapshot();
    let mut entry = text_entry(1, "prompt");
    let selected_end = entry.text.len() as u64;
    entry.text.push_str(" Read other.");
    entry.raw_block =
        serde_json::value::to_raw_value(&serde_json::json!({"type":"text","text":entry.text}))
            .unwrap();
    let s::ReadSource::Event(source) = &mut entry.source else {
        unreachable!()
    };
    source.block_id.run = "prior-run".into();
    source.envelope.run = "prior-run".into();
    entry.selected_span = Some(s::SelectedSpan {
        span_ref: snapshot.task.prompt.clone(),
        start: 0.into(),
        end: selected_end.into(),
    });
    let span = snapshot.task.prompt.clone();
    let units = fx.material(
        s::ReadSelector::Span {
            seed_id: span.seed_id,
            local_id: span.local_id,
            revision: span.revision,
        },
        vec![entry],
    );
    assert_eq!(
        units[0].messages()[0].content,
        vec![ContentBlock::Text {
            text: "Read prompt.".into()
        }]
    );
    fx.turn("partial-prompt");
    fx.generation(units, 3, 0, "", Finish::Completed);
    let before = fx.records(Kind::StateMarkPosition).len();
    let event = effect(fx.summary.seed(), "authorize", None);
    let proof = fx.refresh_native(event, snapshot);
    let run = fx.run();
    run.context
        .begin_place_effect_marks(proof, &mut run.clock)
        .unwrap();
    assert!(fx.finish_native().positions().is_empty());
    assert_eq!(
        fx.records(Kind::StateMarkPosition).len(),
        before,
        "the valid placed partial span is not a native whole-Text occurrence"
    );
}

#[test]
fn place_cancelled_unsent_task_does_not_open_scope_or_block_a_later_real_task() {
    let (mut fx, certificate) = task_place_fixture();
    let first = actual_task_at(&mut fx, &certificate, 1);
    let run = fx.run();
    let units = run
        .context
        .hold_task_material(first, &mut run.clock)
        .unwrap()
        .into_units();
    let grant = fx.grant(units, 3);
    fx.start_generation(grant);
    let run = &mut fx.summary.boot.member.run;
    run.context
        .cancel_generation(&mut fx.decode, &mut run.clock)
        .unwrap();
    assert!(matches!(
        fx.finish_generation(),
        GenerationCompletion::Cancelled(_)
    ));
    assert_eq!(fx.records(Kind::StateMarkPosition).len(), 1);
    assert_eq!(fx.journal().6, 0);
    assert!(receive(fx.peer.as_fd()).is_none());
    fx.close_turn();
    let second = actual_task_at(&mut fx, &certificate, 10);
    let prompt = second.scope().prompt.clone();
    let run = fx.run();
    let units = run
        .context
        .hold_task_material(second, &mut run.clock)
        .unwrap()
        .into_units();
    let proof = fx.generation(units, 3, 0, "", Finish::Completed);
    assert_eq!(proof.generation().resident, fx.journal().1);
    assert_eq!(fx.records(Kind::StateMarkPosition).len(), 2);
    assert_eq!(
        fx.records(Kind::StateMarkPosition).last().unwrap()["payload"]["span"],
        serde_json::to_value(prompt).unwrap()
    );
}

#[test]
fn place_cut_drains_actual_native_effect_records_without_changing_its_requested_boundary() {
    let (mut fx, plan) = PlaceFixture::with_active_scope();
    let units = fx.whole_span(8, "record", 80);
    fx.turn("scope-work");
    fx.generation(units, 3, 0, "", Finish::Completed);
    let proof = fx.closed_effect(plan);
    let mark = fx.mark(80);
    fx.close_turn();
    let prefix = fx.prefix(Some(mark.clone()), s::CutMethod::Flush);
    let n = prefix.n();
    let before = prefix.resident_before();
    let rebuild = prefix.rebuild();
    fx.begin_reload(prefix, ReloadTail::held(vec![]));
    fx.advance_reload().unwrap();
    let cut_bound = fx.run().context.deadlines.get(PumpDeadline::Cut).unwrap();
    assert!(
        receive(fx.peer.as_fd()).is_none(),
        "refreshed summary still owes actual closing marks"
    );
    let run = fx.run();
    assert_eq!(
        run.context
            .prepare_prefix(None, s::CutMethod::Flush, &mut run.clock)
            .unwrap_err(),
        PlaceError::Busy
    );
    run.context
        .begin_place_effect_marks(proof, &mut run.clock)
        .unwrap();
    fx.summary.boot.member.fill_trace();
    let mut pending = None;
    for _ in 0..128 {
        native_marks_pressure(&mut fx);
        if let Some(author) = fx.run().context.pending_author() {
            pending = Some(author.ticket);
            break;
        }
    }
    let pending = pending.expect("real owed native position under recorder pressure");
    for _ in 0..4 {
        native_reload_pressure(&mut fx);
        assert_eq!(fx.run().context.pending_author().unwrap().ticket, pending);
        assert_eq!(
            fx.run().context.deadlines.get(PumpDeadline::Cut),
            Some(cut_bound)
        );
        assert!(
            receive(fx.peer.as_fd()).is_none(),
            "native mark registration without Recorded cannot open cut mutation"
        );
    }
    fx.summary.drain();
    assert_eq!(fx.finish_native().recorded_sequences().len(), 2);
    let TokenDirective::MeasureDelta { keep, delta, .. } = fx.reload_packet() else {
        panic!("completed native drain must permit the actual measure");
    };
    assert_eq!(keep, n);
    assert_eq!(delta, rebuild);
    assert_eq!(
        fx.run().context.deadlines.get(PumpDeadline::Cut),
        Some(cut_bound)
    );
    fx.answer(&fx.measured(keep, 3));
    assert_eq!(fx.reload_packet(), TokenDirective::Flush { keep });
    fx.answer(&TokenAnswer::Flushed {
        resident_before: before,
        resident_after: keep,
    });
    assert!(matches!(fx.finish_reload(), ReloadCompletion::Staged(_)));
    let record = fx.records(Kind::StatePlacement).pop().unwrap();
    assert_eq!(
        record["payload"]["cut"],
        serde_json::to_value(mark).unwrap()
    );
    assert_eq!(fx.journal().1, n);
    assert!(fx.run().context.failure().is_none());
}

#[test]
fn place_native_operator_prompt_keeps_original_filed_source_through_reload_carry() {
    let mut fx = PlaceFixture::new();
    let units = fx.text(8, "seed");
    fx.turn("filed-prompt-source");
    fx.hold_files = true;
    let mut generated = fx.generation(units, 3, 1, "Read prompt.", Finish::Completed);
    let mut output = generated.take_output().unwrap();
    let source = output.source().block_id.clone();
    let lease = output.take_receipt_lease().unwrap();
    let pending = fx
        .run()
        .context
        .hold_reload_text(lease, source.clone())
        .unwrap();
    fx.close_turn();
    let boot = fx
        .records(Kind::StateMarkPosition)
        .into_iter()
        .next()
        .unwrap();
    let cut = s::CutReference {
        span: serde_json::from_value(boot["payload"]["span"].clone()).unwrap(),
        occurrence_id: serde_json::from_value(boot["payload"]["id"].clone()).unwrap(),
    };
    let prefix = fx.prefix(Some(cut), s::CutMethod::Flush);
    let mut tail = ReloadTail::held(vec![]);
    tail.push_pending(pending);
    fx.hold_files = false;
    let staged = fx.staged(prefix, tail, 3);
    fx.turn("filed-prompt-carry");
    fx.begin_staged(staged, vec![]);
    let TokenDirective::MeasureDelta { keep, .. } = fx.next_measure() else {
        unreachable!()
    };
    fx.answer(&fx.measured(keep, 3));
    let PlacementCompletion::Granted(grant) = fx.take() else {
        panic!("actual filed carry refused");
    };
    fx.start_generation(grant);
    fx.sent_generation();
    fx.answer(&TokenAnswer::Generated(super::generated(
        keep,
        3,
        0,
        "",
        Finish::Completed,
        fx.journal().2,
    )));
    assert!(matches!(
        fx.finish_generation(),
        GenerationCompletion::Generated(_)
    ));
    let mut snapshot = fx.summary.snapshot();
    snapshot.operator_source = source.clone();
    let mut event = effect(fx.summary.seed(), "authorize", None);
    let Some(Payload::PlanEffect(record)) = &mut event.payload else {
        unreachable!()
    };
    let t::PlanEffect::Authorize { source: prompt, .. } = &mut record.context else {
        unreachable!()
    };
    *prompt = crate::flow::mirror::source(&source);
    let proof = fx.refresh_native(event, snapshot);
    let run = fx.run();
    run.context
        .begin_place_effect_marks(proof, &mut run.clock)
        .unwrap();
    let done = fx.finish_native();
    assert_eq!(done.positions().len(), 1);
    let placed = fx.records(Kind::StatePlacement).pop().unwrap();
    assert_eq!(
        done.positions()[0].end,
        t::ArgumentCursor::Message {
            message_id: serde_json::from_value::<weaver_types::state_flow::DecimalU64>(
                placed["payload"]["message_ids"][0].clone(),
            )
            .unwrap()
            .0
            .into(),
            block_end: 1.into(),
            byte_end: None
        }
    );
    assert_eq!(
        fx.summary
            .files
            .iter()
            .filter(|file| file.block_id == source)
            .count(),
        1,
        "native prompt uses its one original Filed receipt, not a fabricated second file"
    );
}

#[test]
fn place_failed_late_generated_keeps_retained_native_admission_and_all_owed_trace_facts() {
    let (mut fx, certificate) = task_place_fixture();
    let task = actual_task(&mut fx, &certificate);
    let prompt = task.scope().prompt.clone();
    let run = fx.run();
    let units = run
        .context
        .hold_task_material(task, &mut run.clock)
        .unwrap()
        .into_units();
    let before = fx.journal();
    let grant = fx.grant(units, 3);
    fx.start_generation(grant);
    let TokenDirective::AppendAndGenerate { turn, .. } = fx.sent_generation() else {
        unreachable!()
    };
    let run = fx.run();
    let first = run.context.fail(
        &mut run.clock,
        FlowHaltReason::ProtocolFailed,
        HaltPoint::Barrier,
        "unrelated component failed after complete Append send",
    );
    assert_eq!(run.context.phase(), RecordingPhase::Failed);
    assert!(run.context.committed_plan_summary().is_err());
    assert_eq!(
        fx.account_step().unwrap(),
        PlaceProgress::Pending,
        "actual first fault attempt must precede owed account"
    );
    fx.first_fault_attempt();
    let mut cancel = None;
    for _ in 0..32 {
        fx.account_step().unwrap();
        cancel = receive(fx.peer.as_fd());
        if cancel.is_some() {
            break;
        }
    }
    assert_eq!(
        serde_json::from_slice::<TokenDirective>(
            &cancel.expect("original sent generation must receive its Cancel")
        )
        .unwrap(),
        TokenDirective::Cancel { turn }
    );
    let generated = generated(before.1, 3, 0, "", Finish::Stopped, before.2);
    let resident = generated.resident;
    let request: serde_json::Value = serde_json::from_str(generated.request.get()).unwrap();
    let measurement: serde_json::Value = serde_json::from_str(generated.measurement.get()).unwrap();
    let raw = fx.answer(&TokenAnswer::Generated(generated));
    fx.answer(&TokenAnswer::AtRest);
    let account = fx.settled_account();
    assert!(
        account.incomplete().is_none(),
        "valid late terminal is a complete truthful account"
    );
    assert_eq!(account.raw_bytes(), Some(raw.as_slice()));
    assert_eq!(
        account
            .confirmed_generation()
            .unwrap()
            .generation()
            .resident,
        resident
    );
    assert_eq!(fx.journal().1, resident);
    assert_eq!(fx.journal().6, 2);
    assert_eq!(
        fx.records(Kind::ModelRequest).last().unwrap()["payload"],
        request
    );
    assert_eq!(
        fx.records(Kind::ModelMeasurement).last().unwrap()["payload"],
        measurement
    );
    assert_eq!(fx.records(Kind::ModelOutput).len(), 1);
    assert_eq!(fx.records(Kind::StateMarkPosition).len(), 2);
    assert_eq!(
        fx.records(Kind::StateMarkPosition).last().unwrap()["payload"]["span"],
        serde_json::to_value(prompt).unwrap()
    );
    let fault: u64 = fx.records(Kind::Fault)[0]["sequence"]
        .as_str()
        .unwrap()
        .parse()
        .unwrap();
    for kind in [
        Kind::ModelRequest,
        Kind::ModelOutput,
        Kind::ModelMeasurement,
        Kind::StateMarkPosition,
    ] {
        let sequence: u64 = fx.records(kind).last().unwrap()["sequence"]
            .as_str()
            .unwrap()
            .parse()
            .unwrap();
        assert!(
            sequence > fault,
            "late fact follows the actual first fault attempt"
        );
        assert!(
            account
                .recorded_sequences()
                .iter()
                .any(|held| held.0 == sequence)
        );
    }
    assert_eq!(
        fx.records(Kind::MessageAssistant).len(),
        0,
        "stopped evidence cannot authorize assistant content filing"
    );
    assert_eq!(fx.run().context.failure(), Some(&first));
    assert!(fx.run().context.committed_plan_summary().is_err());
    assert!(fx.run().context.completed_author.is_none());
    assert!(receive(fx.peer.as_fd()).is_none());
    let run = fx.run();
    assert_eq!(
        run.context
            .prepare_prefix(None, s::CutMethod::Flush, &mut run.clock)
            .unwrap_err(),
        PlaceError::WrongPhase
    );
}

#[test]
fn place_refused_queued_cut_keeps_native_handoff_and_response_behind_its_real_record() {
    let (mut fx, plan) = PlaceFixture::with_active_scope();
    let units = fx.whole_span(8, "record", 80);
    fx.turn("cut-old-mark");
    fx.generation(units, 3, 0, "", Finish::Completed);
    let old_mark = fx.mark(80);
    let stale = fx.prefix(Some(old_mark), s::CutMethod::Flush);
    fx.close_turn();
    let boot = fx
        .records(Kind::StateMarkPosition)
        .into_iter()
        .next()
        .unwrap();
    let boot_cut = s::CutReference {
        span: serde_json::from_value(boot["payload"]["span"].clone()).unwrap(),
        occurrence_id: serde_json::from_value(boot["payload"]["id"].clone()).unwrap(),
    };
    let prefix = fx.prefix(Some(boot_cut), s::CutMethod::Flush);
    let staged = fx.staged(prefix, ReloadTail::held(vec![]), 0);
    fx.turn("cut-after-removal");
    fx.begin_staged(staged, vec![]);
    let TokenDirective::MeasureDelta { keep, delta, .. } = fx.next_measure() else {
        unreachable!()
    };
    assert!(delta.is_empty());
    fx.answer(&fx.measured(keep, 0));
    let PlacementCompletion::Granted(grant) = fx.take() else {
        panic!("actual empty staged continuation refused");
    };
    fx.start_generation(grant);
    fx.sent_generation();
    fx.answer(&TokenAnswer::Generated(generated(
        keep,
        0,
        0,
        "",
        Finish::Completed,
        fx.journal().2,
    )));
    assert!(matches!(
        fx.finish_generation(),
        GenerationCompletion::Generated(_)
    ));
    let proof = fx.closed_effect(plan);
    let proof_sequence = proof.filing().sequence();
    let positions = fx.records(Kind::StateMarkPosition).len();
    fx.begin_reload(stale, ReloadTail::held(vec![]));
    fx.close_turn();
    fx.advance_reload().unwrap();
    let bound = fx.run().context.deadlines.get(PumpDeadline::Cut).unwrap();
    let run = fx.run();
    assert!(
        !run.context.reload_draining(),
        "a queued refusal is no longer a source-draining cut"
    );
    assert!(
        run.context.place_close().is_none(),
        "response is unavailable before its owed refusal records"
    );
    assert_eq!(
        run.context.place_response_sent(&mut run.clock),
        Err(RunFailure::PlaceOwned)
    );
    let error = run
        .context
        .begin_place_effect_marks(proof, &mut run.clock)
        .unwrap_err();
    assert_eq!(error.refusal, PlaceError::Busy);
    assert!(error.failure.is_none());
    assert_eq!(error.prerequisite.filing().sequence(), proof_sequence);
    fx.summary.boot.member.fill_trace();
    let mut pending = None;
    for _ in 0..128 {
        native_reload_pressure(&mut fx);
        if let Some(author) = fx.run().context.pending_author() {
            pending = Some(author.ticket);
            break;
        }
    }
    let pending = pending.expect("actual stale-cut refusal must own its recorder ticket");
    let run = fx.run();
    assert!(run.context.place_close().is_none());
    assert_eq!(
        run.context.place_response_sent(&mut run.clock),
        Err(RunFailure::PlaceOwned)
    );
    let error = run
        .context
        .begin_place_effect_marks(error.prerequisite, &mut run.clock)
        .unwrap_err();
    assert_eq!(error.refusal, PlaceError::Busy);
    assert!(error.failure.is_none());
    assert_eq!(run.context.pending_author().unwrap().ticket, pending);
    assert_eq!(run.context.deadlines.get(PumpDeadline::Cut), Some(bound));
    assert!(run.context.native_marks.is_none());
    assert_eq!(fx.records(Kind::StateMarkPosition).len(), positions);
    assert!(receive(fx.peer.as_fd()).is_none());
    fx.summary.drain();
    let ReloadCompletion::Refused(account) = fx.finish_reload() else {
        panic!("removed occurrence must refuse without a second cut");
    };
    assert_eq!(
        account.reload_refusal().map(|(code, _)| code),
        Some(ReloadCode::InvalidPrefix)
    );
    assert_eq!(account.recorded_sequences().len(), 1);
    let run = fx.run();
    assert!(
        run.context.place_close().is_some(),
        "actual refusal completion makes the response available"
    );
    run.context.place_response_sent(&mut run.clock).unwrap();
    run.context
        .begin_place_effect_marks(error.prerequisite, &mut run.clock)
        .unwrap();
    assert_eq!(fx.finish_native().positions().len(), 2);
    assert!(fx.run().context.completed_author.is_none());
    assert!(fx.run().context.failure().is_none());
}

// This helper deliberately stops before summary refresh. The real committed
// effect is already owed when a distinct, actual reload starts its Cut wait.
fn pending_summary_cut(
    cut_first: bool,
) -> (
    PlaceFixture,
    FilingCompletion,
    s::PlanSnapshot,
    AbsoluteBound,
    AbsoluteBound,
) {
    let (mut fx, plan) = PlaceFixture::with_active_scope();
    let prefix = fx.prefix(None, s::CutMethod::Flush);
    let (filing, plan) = fx.closed_filing(plan);
    let decision = filing.wait_bound();
    assert_eq!(filing.wait_key(), PumpDeadline::Decision);
    assert!(fx.run().context.open_turn.is_none());
    fx.run().context.declaration.cut_wait.elapsed_ns = if cut_first {
        500_000.into()
    } else {
        2_000_000.into()
    };
    fx.hold_files = true;
    fx.begin_reload(prefix, ReloadTail::held(vec![]));
    fx.advance_reload().unwrap();
    let cut = fx.run().context.deadlines.get(PumpDeadline::Cut).unwrap();
    assert_eq!(cut.deadline_ns() < decision.deadline_ns(), cut_first);
    assert_eq!(fx.run().context.reload_drain_bound(), Some(cut));
    assert!(receive(fx.peer.as_fd()).is_none());
    (fx, filing, plan, decision, cut)
}

fn assert_summary_cut_waits(fx: &mut PlaceFixture, decision: AbsoluteBound, cut: AbsoluteBound) {
    let run = fx.run();
    assert_eq!(
        run.context.deadlines.get(PumpDeadline::Decision),
        Some(decision)
    );
    assert_eq!(run.context.deadlines.get(PumpDeadline::Cut), Some(cut));
    assert_eq!(run.context.complete_wait(PumpDeadline::Decision), None);
    assert_eq!(run.context.complete_wait(PumpDeadline::Cut), None);
    assert!(
        receive(fx.peer.as_fd()).is_none(),
        "an owed summary cannot authorize any decode request"
    );
}

#[test]
fn place_cut_drains_actual_decision_summary_through_recorded_without_replacing_either_wait() {
    for cut_first in [true, false] {
        let (mut fx, filing, plan, decision, cut) = pending_summary_cut(cut_first);
        let source = committed_position(&filing);
        let filing_sequence = filing.sequence();
        let before = fx.journal();
        fx.summary.begin_effect(filing);
        let raw = fx.summary.release(Some(plan));
        assert_eq!(
            fx.summary.boot.member.bound().deadline_ns(),
            decision.deadline_ns().min(cut.deadline_ns())
        );
        assert_summary_cut_waits(&mut fx, decision, cut);

        fx.summary.boot.member.record_completion_only();
        let run = fx.run();
        run.context.settle_recorded_read(&mut run.clock).unwrap();
        assert!(run.context.read_blocks_reduction());
        fx.summary.boot.member.fill_trace();
        let recall_count = fx.records(Kind::Recall).len();
        assert!(matches!(
            native_pressure(&mut fx, |fx| fx.summary.advance()),
            None | Some(PlanSummaryProgress::Pending)
        ));
        let ticket = fx.run().context.pending_author().unwrap().ticket;
        for _ in 0..4 {
            native_reload_pressure(&mut fx);
            assert!(matches!(
                native_pressure(&mut fx, |fx| fx.summary.advance()),
                None | Some(PlanSummaryProgress::Pending)
            ));
            assert_eq!(fx.run().context.pending_author().unwrap().ticket, ticket);
            assert_eq!(fx.records(Kind::Recall).len(), recall_count);
            assert!(fx.run().context.committed_plan_summary().is_err());
            assert_summary_cut_waits(&mut fx, decision, cut);
        }
        fx.summary.drain();
        let PlanSummaryCompletion::Effect(proof) = fx.summary.finish() else {
            panic!("owed Decision refresh did not finish under Cut");
        };
        assert_eq!(proof.filing().sequence(), filing_sequence);
        assert_eq!(committed_position(proof.filing()), source);
        assert_eq!(proof.read().raw_bytes(), raw);
        assert_eq!(fx.records(Kind::Recall).len(), recall_count + 1);
        assert_eq!(
            fx.run().context.deadlines.get(PumpDeadline::Decision),
            Some(decision)
        );
        assert_eq!(fx.run().context.deadlines.get(PumpDeadline::Cut), Some(cut));
        fx.advance_reload().unwrap();
        assert!(
            receive(fx.peer.as_fd()).is_none(),
            "Recorded summary still owes its exact native handoff"
        );
        let run = fx.run();
        run.context
            .begin_place_effect_marks(proof, &mut run.clock)
            .unwrap();
        assert!(
            fx.finish_native().positions().is_empty(),
            "unobserved scopes do not invent native ends"
        );
        let TokenDirective::MeasureDelta { keep, delta, .. } = fx.reload_packet() else {
            panic!("cut must first measure its exact original prefix");
        };
        assert_eq!(keep, before.1);
        assert!(delta.is_empty());
        assert_eq!(fx.run().context.deadlines.get(PumpDeadline::Cut), Some(cut));
        fx.answer(&fx.measured(keep, 0));
        assert_eq!(fx.reload_packet(), TokenDirective::Flush { keep });
        assert_eq!(fx.run().context.deadlines.get(PumpDeadline::Cut), Some(cut));
        fx.answer(&TokenAnswer::Flushed {
            resident_before: before.1,
            resident_after: keep,
        });
        assert!(matches!(fx.finish_reload(), ReloadCompletion::Staged(_)));
        assert_eq!(fx.journal().1, keep);
        assert!(fx.run().context.failure().is_none());
    }
}

#[test]
fn place_cut_summary_read_expires_at_the_earlier_original_decision_or_cut() {
    for cut_first in [true, false] {
        let (mut fx, filing, _plan, decision, cut) = pending_summary_cut(cut_first);
        let placements = fx.records(Kind::StatePlacement).len();
        fx.summary.begin_effect(filing);
        let request = fx.summary.next_request();
        assert!(matches!(
            request,
            s::StateFrame::Control {
                frame: s::StateControl::Read {
                    selector: s::ReadSelector::PlanCurrent {}
                },
                ..
            }
        ));
        let bound = fx.summary.boot.member.bound();
        let earliest = decision.deadline_ns().min(cut.deadline_ns());
        assert_eq!(bound.deadline_ns(), earliest);
        fx.run().clock.advance_to(earliest).unwrap();
        let first = fx.summary.advance_failure();
        assert_eq!(fx.run().context.failure(), Some(&first));
        assert!(
            fx.run().context.plan_summary.is_some(),
            "the original effect owner remains available for stopped accounting"
        );
        assert_eq!(fx.run().context.read.as_ref().unwrap().plan_bound(), bound);
        assert!(fx.run().context.committed_plan_summary().is_err());
        let run = fx.run();
        assert!(run.context.take_plan_summary(&mut run.clock).is_err());
        assert!(receive(fx.peer.as_fd()).is_none());
        assert_eq!(fx.records(Kind::StatePlacement).len(), placements);
    }
}

#[test]
fn place_cut_summary_does_not_borrow_a_naked_cut_or_unrelated_wait() {
    let (mut fx, plan) = PlaceFixture::with_active_scope();
    let (filing, plan) = fx.closed_filing(plan);
    let original = filing.wait_bound();
    let sequence = filing.sequence();
    let run = fx.run();
    let naked = AbsoluteBound::after(run.clock.now().unwrap(), 500_000).unwrap();
    run.context
        .arm(PumpDeadline::Cut, naked, &mut run.clock)
        .unwrap();
    let error = run
        .context
        .begin_effect_plan_summary(filing, &mut run.clock)
        .unwrap_err();
    assert_eq!(error.refusal, PlanSummaryBeginRefusal::Busy);
    assert!(error.failure.is_none());
    assert_eq!(error.prerequisite.sequence(), sequence);
    assert_eq!(error.prerequisite.wait_bound(), original);
    assert!(run.context.plan_summary.is_none());
    assert!(run.context.read.is_none());
    assert!(run.context.flow.state().outstanding().is_empty());
    assert_eq!(run.context.complete_wait(PumpDeadline::Cut), Some(naked));
    fx.summary.begin_effect(error.prerequisite);
    fx.summary.release(Some(plan));
    assert!(matches!(
        fx.summary.finish(),
        PlanSummaryCompletion::Effect(_)
    ));

    for key in [PumpDeadline::Gate, PumpDeadline::StateControl] {
        let (mut fx, filing, plan, decision, cut) = pending_summary_cut(true);
        let sequence = filing.sequence();
        let run = fx.run();
        let foreign = AbsoluteBound::after(run.clock.now().unwrap(), 100_000).unwrap();
        run.context.arm(key, foreign, &mut run.clock).unwrap();
        let error = run
            .context
            .begin_effect_plan_summary(filing, &mut run.clock)
            .unwrap_err();
        assert_eq!(error.refusal, PlanSummaryBeginRefusal::Busy);
        assert!(error.failure.is_none());
        assert_eq!(error.prerequisite.sequence(), sequence);
        assert_eq!(error.prerequisite.wait_bound(), decision);
        assert!(run.context.plan_summary.is_none());
        assert!(run.context.read.is_none());
        assert!(run.context.flow.state().outstanding().is_empty());
        assert_eq!(run.context.deadlines.get(PumpDeadline::Cut), Some(cut));
        assert_eq!(run.context.complete_wait(key), Some(foreign));
        fx.summary.begin_effect(error.prerequisite);
        fx.summary.release(Some(plan));
        assert!(matches!(
            fx.summary.finish(),
            PlanSummaryCompletion::Effect(_)
        ));
    }
}

#[test]
fn place_cut_drain_does_not_authorize_an_ordinary_read_after_summary_recorded() {
    let (mut fx, plan) = PlaceFixture::with_active_scope();
    let prefix = fx.prefix(None, s::CutMethod::Flush);
    let proof = fx.closed_effect(plan);
    fx.hold_files = true;
    fx.begin_reload(prefix, ReloadTail::held(vec![]));
    fx.advance_reload().unwrap();
    let run = fx.run();
    assert!(run.context.reload_drain_bound().is_some());
    assert!(
        !run.context.plan_summary_blocks_external(),
        "this denial must not depend on an outstanding summary owner"
    );
    let selector = s::ReadSelector::PlanCurrent {};
    let error = run
        .context
        .begin_read(selector.clone(), None, &mut run.clock)
        .unwrap_err();
    assert_eq!(error.selector, selector);
    assert!(error.turn.is_none());
    assert_eq!(error.refusal, read_driver::ReadBeginRefusal::Busy);
    assert!(error.failure.is_none());
    assert!(run.context.read.is_none());
    assert!(run.context.flow.state().outstanding().is_empty());
    assert!(
        run.context
            .deadlines
            .get(PumpDeadline::StateControl)
            .is_none()
    );
    fx.summary.boot.member.no_request();
    let run = fx.run();
    run.context
        .begin_place_effect_marks(proof, &mut run.clock)
        .unwrap();
    fx.finish_native();
}

#[test]
fn place_cancelled_cut_does_not_admit_an_owed_summary_until_reload_is_retired() {
    let (mut fx, filing, plan, decision, cut) = pending_summary_cut(true);
    let sequence = filing.sequence();
    let run = fx.run();
    run.context.cancel_reload(&mut run.clock).unwrap();
    assert!(run.context.reload_drain_bound().is_none());
    let error = run
        .context
        .begin_effect_plan_summary(filing, &mut run.clock)
        .unwrap_err();
    assert_eq!(error.refusal, PlanSummaryBeginRefusal::Busy);
    assert!(error.failure.is_none());
    assert_eq!(error.prerequisite.sequence(), sequence);
    assert_eq!(error.prerequisite.wait_bound(), decision);
    assert!(run.context.plan_summary.is_none());
    assert!(run.context.read.is_none());
    assert!(run.context.flow.state().outstanding().is_empty());
    assert_eq!(run.context.deadlines.get(PumpDeadline::Cut), Some(cut));
    assert!(matches!(fx.finish_reload(), ReloadCompletion::Cancelled(_)));
    assert!(fx.run().context.deadlines.get(PumpDeadline::Cut).is_none());
    fx.summary.begin_effect(error.prerequisite);
    fx.summary.release(Some(plan));
    assert!(matches!(
        fx.summary.finish(),
        PlanSummaryCompletion::Effect(_)
    ));
    assert!(receive(fx.peer.as_fd()).is_none());
}

#[test]
fn place_summary_borrow_rejects_removed_or_replaced_actual_reload_bound() {
    for replace in [false, true] {
        let (mut fx, filing, _plan, decision, cut) = pending_summary_cut(true);
        let sequence = filing.sequence();
        let run = fx.run();
        // Explicit corruption defense: an erroneous internal reducer bypasses
        // complete_wait, which correctly refuses this actual Cut owner.
        assert_eq!(run.context.complete_wait(PumpDeadline::Cut), None);
        assert_eq!(run.context.deadlines.disarm(PumpDeadline::Cut), Some(cut));
        if replace {
            let other = AbsoluteBound::after(run.clock.now().unwrap(), 2_000_000).unwrap();
            run.context
                .arm(PumpDeadline::Cut, other, &mut run.clock)
                .unwrap();
        }
        assert!(run.context.reload_drain_bound().is_none());
        let error = run
            .context
            .begin_effect_plan_summary(filing, &mut run.clock)
            .unwrap_err();
        assert_eq!(error.refusal, PlanSummaryBeginRefusal::Busy);
        assert!(error.failure.is_none());
        assert_eq!(error.prerequisite.sequence(), sequence);
        assert_eq!(error.prerequisite.wait_bound(), decision);
        assert!(run.context.plan_summary.is_none());
        assert!(run.context.read.is_none());
        assert!(run.context.flow.state().outstanding().is_empty());
        let failure = fx.advance_reload().unwrap_err();
        assert_eq!(fx.run().context.failure(), Some(&failure));
        assert!(receive(fx.peer.as_fd()).is_none());
    }
}

#[test]
fn place_refused_cut_does_not_admit_actual_owed_summary_before_its_refusal_records() {
    let (mut fx, plan) = PlaceFixture::with_active_scope();
    let units = fx.whole_span(8, "record", 80);
    fx.turn("summary-cut-old-mark");
    fx.generation(units, 3, 0, "", Finish::Completed);
    let mark = fx.mark(80);
    let stale = fx.prefix(Some(mark), s::CutMethod::Flush);
    fx.close_turn();
    let boot = fx
        .records(Kind::StateMarkPosition)
        .into_iter()
        .next()
        .unwrap();
    let boot_cut = s::CutReference {
        span: serde_json::from_value(boot["payload"]["span"].clone()).unwrap(),
        occurrence_id: serde_json::from_value(boot["payload"]["id"].clone()).unwrap(),
    };
    let prefix = fx.prefix(Some(boot_cut), s::CutMethod::Flush);
    let staged = fx.staged(prefix, ReloadTail::held(vec![]), 0);
    fx.turn("summary-cut-after-removal");
    fx.begin_staged(staged, vec![]);
    let TokenDirective::MeasureDelta { keep, delta, .. } = fx.next_measure() else {
        unreachable!()
    };
    assert!(delta.is_empty());
    fx.answer(&fx.measured(keep, 0));
    let PlacementCompletion::Granted(grant) = fx.take() else {
        panic!("actual staged continuation refused");
    };
    fx.start_generation(grant);
    fx.sent_generation();
    fx.answer(&TokenAnswer::Generated(generated(
        keep,
        0,
        0,
        "",
        Finish::Completed,
        fx.journal().2,
    )));
    assert!(matches!(
        fx.finish_generation(),
        GenerationCompletion::Generated(_)
    ));
    // Queue while this real turn is open; its old address is checked only at
    // the between-turn pause. The effect then becomes owed before Cut starts.
    fx.begin_reload(stale, ReloadTail::held(vec![]));
    fx.close_turn();
    let (filing, _plan) = fx.closed_filing(plan);
    let sequence = filing.sequence();
    let decision = filing.wait_bound();
    fx.hold_files = true;
    fx.advance_reload().unwrap();
    let run = fx.run();
    let cut = run.context.deadlines.get(PumpDeadline::Cut).unwrap();
    assert!(!run.context.reload_draining());
    assert!(run.context.reload_drain_bound().is_none());
    assert!(run.context.place_close().is_none());
    let error = run
        .context
        .begin_effect_plan_summary(filing, &mut run.clock)
        .unwrap_err();
    assert_eq!(error.refusal, PlanSummaryBeginRefusal::Busy);
    assert!(error.failure.is_none());
    assert_eq!(error.prerequisite.sequence(), sequence);
    assert_eq!(error.prerequisite.wait_bound(), decision);
    assert!(run.context.plan_summary.is_none());
    assert!(run.context.read.is_none());
    assert!(run.context.flow.state().outstanding().is_empty());
    assert_eq!(run.context.deadlines.get(PumpDeadline::Cut), Some(cut));
    assert!(receive(fx.peer.as_fd()).is_none());
}

fn accepted_cut_input(fx: &mut PlaceFixture) -> crate::flow::input::AcceptedInput {
    let run = fx.run();
    run.context
        .begin_input(
            weaver_types::TurnFrame::carry_from(br#"{"text":"Read prompt."}"#, 42),
            &mut run.clock,
        )
        .unwrap();
    for _ in 0..128 {
        let run = fx.run();
        run.context
            .advance_input(&mut ports(&mut run.clock))
            .unwrap();
        fx.classifier(false);
        let run = fx.run();
        if let Some(token) = run.context.take_accepted_input(&mut run.clock).unwrap() {
            return token;
        }
    }
    panic!("actual accepted input did not transfer");
}

fn queued_cut_before_task_input(queue_first: bool) -> (PlaceFixture, AbsoluteBound) {
    let (mut fx, certificate) = task_place_fixture();
    let original = fx.decision();
    let mut prefix = Some(fx.prefix(None, s::CutMethod::Flush));
    let token = accepted_cut_input(&mut fx);
    if queue_first {
        fx.begin_reload(prefix.take().unwrap(), ReloadTail::held(vec![]));
    }
    let run = fx.run();
    run.context
        .begin_task(
            token,
            weaver_trace::TurnRef("queued-cut-task".into()),
            &certificate,
            &mut run.clock,
        )
        .unwrap();
    if !queue_first {
        fx.begin_reload(prefix.take().unwrap(), ReloadTail::held(vec![]));
    }
    fx.hold_files = true; // task_member owns both File and control replies below.
    assert!(fx.run().context.marks.is_some());
    assert!(fx.run().context.open_turn.is_none());
    assert!(fx.run().context.input_turn.is_none());
    assert!(fx.records(Kind::TurnStarted).is_empty());
    (fx, original)
}

#[test]
fn place_queued_cut_preserves_actual_pre_input_task_until_its_turn_closes() {
    for queue_first in [false, true] {
        let (mut fx, original) = queued_cut_before_task_input(queue_first);
        for _ in 0..3 {
            assert_eq!(fx.advance_reload().unwrap(), ReloadProgress::Pending);
            assert!(fx.run().context.deadlines.get(PumpDeadline::Cut).is_none());
            assert_eq!(
                fx.run().context.deadlines.get(PumpDeadline::Decision),
                Some(original)
            );
            assert!(fx.records(Kind::TurnStarted).is_empty());
            assert!(receive(fx.peer.as_fd()).is_none());
        }
        let mut completed = None;
        for _ in 0..256 {
            let run = fx.run();
            run.context
                .advance_task(&mut ports(&mut run.clock))
                .unwrap();
            fx.classifier(false);
            task_member(&mut fx, 1);
            let run = fx.run();
            if let Some(done) = run.context.take_task(&mut run.clock).unwrap() {
                completed = Some(done);
                break;
            }
            assert_eq!(
                run.context.deadlines.get(PumpDeadline::Decision),
                Some(original)
            );
            assert_eq!(fx.advance_reload().unwrap(), ReloadProgress::Pending);
            assert!(fx.run().context.deadlines.get(PumpDeadline::Cut).is_none());
            assert!(receive(fx.peer.as_fd()).is_none());
        }
        let TaskCompletion::Begun(task) =
            completed.expect("original task must finish while Cut remains queued")
        else {
            panic!("actual task refused");
        };
        assert_eq!(fx.records(Kind::TurnStarted).len(), 1);
        assert_eq!(fx.records(Kind::MessageUser).len(), 1);
        assert_eq!(
            fx.summary
                .files
                .iter()
                .filter(|file| &file.block_id == task.input().source())
                .count(),
            1
        );
        assert!(fx.run().context.marks.is_none());
        assert_eq!(
            fx.run().context.open_turn.as_ref().unwrap().0,
            "queued-cut-task"
        );
        assert_eq!(fx.advance_reload().unwrap(), ReloadProgress::Pending);
        assert!(fx.run().context.deadlines.get(PumpDeadline::Cut).is_none());
        fx.close_turn();
        let closed_at = fx.run().clock.now().unwrap();
        let before = fx.journal().1;
        let TokenDirective::MeasureDelta { keep, delta, .. } = fx.reload_packet() else {
            panic!("cut may measure only after actual task/turn completion");
        };
        assert_eq!(keep, before);
        assert!(delta.is_empty());
        let cut = fx.run().context.deadlines.get(PumpDeadline::Cut).unwrap();
        assert!(cut.began_ns() >= closed_at);
        assert_eq!(
            cut.deadline_ns() - cut.began_ns(),
            fx.run().context.declaration.cut_wait.elapsed_ns.0
        );
        fx.answer(&fx.measured(keep, 0));
        assert_eq!(fx.reload_packet(), TokenDirective::Flush { keep });
        assert_eq!(fx.run().context.deadlines.get(PumpDeadline::Cut), Some(cut));
        fx.answer(&TokenAnswer::Flushed {
            resident_before: before,
            resident_after: keep,
        });
        assert!(matches!(fx.finish_reload(), ReloadCompletion::Staged(_)));
        assert!(fx.run().context.failure().is_none());
    }
}

#[test]
fn place_queued_cut_allows_actual_pre_input_task_cancellation_without_opening_a_turn() {
    let (mut fx, original) = queued_cut_before_task_input(false);
    assert_eq!(fx.advance_reload().unwrap(), ReloadProgress::Pending);
    assert!(fx.run().context.deadlines.get(PumpDeadline::Cut).is_none());
    assert_eq!(
        fx.run().context.deadlines.get(PumpDeadline::Decision),
        Some(original)
    );
    let run = fx.run();
    run.context.cancel_task();
    run.context
        .advance_task(&mut ports(&mut run.clock))
        .unwrap();
    let TaskCompletion::Cancelled(mut account) =
        run.context.take_task(&mut run.clock).unwrap().unwrap()
    else {
        panic!("unconsumed task cancellation must preserve its account");
    };
    let _returned_input = account
        .take_unconsumed_input()
        .expect("the actual Input owner still owns this accepted token");
    assert!(run.context.marks.is_none());
    assert!(run.context.deadlines.get(PumpDeadline::Decision).is_none());
    assert!(run.context.input.snapshot().is_some());
    // Cancelling begin_task returns its unconsumed input; it does not dispose
    // that separate owner. Finish its existing stopped response before Cut.
    run.context.cancel_input(&mut run.clock).unwrap();
    assert_eq!(
        run.context.input_close().unwrap(),
        weaver_types::TurnFrame::carry(br#"{"kind":"stopped","reason":"The agent is leaving."}"#)
    );
    run.context.input_response_sent(&mut run.clock).unwrap();
    run.context.finish_input(&mut run.clock).unwrap();
    assert!(run.context.input.snapshot().is_none());
    assert!(fx.records(Kind::TurnStarted).is_empty());
    assert!(fx.records(Kind::MessageUser).is_empty());
    assert!(matches!(
        fx.reload_packet(),
        TokenDirective::MeasureDelta { .. }
    ));
    assert!(fx.run().context.deadlines.get(PumpDeadline::Cut).is_some());
    assert!(fx.run().context.failure().is_none());
}

#[test]
fn place_queued_cut_retains_pre_input_task_decision_expiry_without_starting_a_cut_clock() {
    let (mut fx, original) = queued_cut_before_task_input(true);
    assert_eq!(fx.advance_reload().unwrap(), ReloadProgress::Pending);
    assert!(fx.run().context.deadlines.get(PumpDeadline::Cut).is_none());
    fx.run().clock.advance_to(original.deadline_ns()).unwrap();
    let first = fx.advance_reload().unwrap_err();
    assert_eq!(fx.run().context.failure(), Some(&first));
    assert!(fx.run().context.marks.is_some());
    assert!(fx.run().context.deadlines.get(PumpDeadline::Cut).is_none());
    assert!(fx.records(Kind::TurnStarted).is_empty());
    assert!(receive(fx.peer.as_fd()).is_none());
}

struct PreTurnCutClock {
    reads: usize,
    before: u64,
    after: u64,
}
impl PumpIo for PreTurnCutClock {
    fn now(&mut self) -> io::Result<u64> {
        if self.reads == 0 {
            Ok(self.after)
        } else {
            self.reads -= 1;
            Ok(self.before)
        }
    }
    fn wait(&mut self, _: &[WaitInterest<'_>], _: Option<u64>) -> io::Result<WaitOutcome> {
        panic!("pre-turn interruption must occur before transport wait");
    }
}

#[test]
fn place_queued_cut_preserves_actual_interrupted_input_turn_before_turn_started() {
    let mut fx = PlaceFixture::new();
    let original = fx.decision();
    let prefix = fx.prefix(None, s::CutMethod::Flush);
    let token = accepted_cut_input(&mut fx);
    let run = fx.run();
    let now = run.clock.now().unwrap();
    let leave = AbsoluteBound::after(now, 5).unwrap();
    run.context
        .arm(PumpDeadline::Leave, leave, &mut run.clock)
        .unwrap();
    let mut io = PreTurnCutClock {
        reads: 2,
        before: now,
        after: leave.deadline_ns(),
    };
    let error = run
        .context
        .consume_input(
            token,
            weaver_trace::TurnRef("queued-cut-input".into()),
            &mut ports(&mut io),
        )
        .unwrap_err();
    assert!(error.token.is_none());
    assert_eq!(
        error.failure,
        RunFailure::Wait {
            key: PumpDeadline::Leave,
            bound: leave
        }
    );
    assert!(run.context.input_turn.is_some());
    assert!(run.context.open_turn.is_none());
    assert!(
        run.context.marks.is_none(),
        "this control independently reaches the input-turn condition"
    );
    assert_eq!(
        run.context.waiting_content.as_ref().unwrap().kind,
        Kind::TurnStarted
    );
    assert_eq!(run.context.complete_wait(PumpDeadline::Leave), Some(leave));
    run.clock.advance_to(leave.deadline_ns()).unwrap();
    fx.begin_reload(prefix, ReloadTail::held(vec![]));
    assert_eq!(fx.advance_reload().unwrap(), ReloadProgress::Pending);
    let run = fx.run();
    assert!(run.context.deadlines.get(PumpDeadline::Cut).is_none());
    assert_eq!(
        run.context.deadlines.get(PumpDeadline::Decision),
        Some(original)
    );
    assert!(run.context.input_turn.is_some());
    assert_eq!(
        run.context.waiting_content.as_ref().unwrap().kind,
        Kind::TurnStarted
    );
    let seated = run
        .context
        .resume_input_turn(&mut ports(&mut run.clock))
        .unwrap()
        .unwrap();
    assert_eq!(seated.turn.0, "queued-cut-input");
    assert_eq!(
        seated.message.content,
        vec![ContentBlock::Text {
            text: "Read prompt.".into()
        }]
    );
    assert_eq!(fx.records(Kind::TurnStarted).len(), 1);
    assert_eq!(fx.records(Kind::MessageUser).len(), 1);
    assert!(fx.run().context.input_turn.is_none());
    assert_eq!(fx.advance_reload().unwrap(), ReloadProgress::Pending);
    assert!(fx.run().context.deadlines.get(PumpDeadline::Cut).is_none());
    assert!(receive(fx.peer.as_fd()).is_none());
    assert!(fx.run().context.failure().is_none());
}

#[path = "context_place_native_return_tests.rs"]
mod return_review_tests;

#[path = "context_place_drain_review_tests.rs"]
mod drain_review_tests;

#[path = "context_place_capability_review_tests.rs"]
mod capability_review_tests;
