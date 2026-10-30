//! Unreviewed closure through the actual read, author, filing and summary
//! owners, with a scripted member. The preview bytes are the store's own pinned
//! encoding (weaver-state work_bytes_d1.rs serves and accepts the same bytes).
use super::*;
use crate::flow::context::plan_port_driver::ClosureEvidence;
use crate::flow::context::{ReadCompletion, RecordedRead};
use crate::flow::templates::{CompiledInventory, TemplateCertificate};

const PREVIEW: &[u8] = include_bytes!("fixtures/work-preview.json");

const _: fn() = || {
    trait NotClone<M> {
        fn pin() {}
    }
    impl<T: ?Sized> NotClone<()> for T {}
    impl<T: Clone> NotClone<u8> for T {}
    let _ = <ClosureEvidence as NotClone<_>>::pin;
};

fn pinned() -> s::WorkRef {
    serde_json::from_str(include_str!("fixtures/work-preview-ref.json")).unwrap()
}

fn work_request() -> PlanRequest {
    let work = pinned();
    PlanRequest::WorkReference {
        expected: work.plan,
        step_id: work.step_id,
        span: work.span,
        through: work.through,
    }
}

/// A locked plan whose first step is active over the pinned work span. With
/// an effect certificate the native scopes are installed for adoption.
fn closure_fixture(review_required: bool, effect: bool) -> (SummaryFixture, TemplateCertificate) {
    static SPECS: std::sync::OnceLock<Vec<s::TemplateSpec>> = std::sync::OnceLock::new();
    let mut fx = if effect {
        let inventory = CompiledInventory::new(SPECS.get_or_init(|| {
            vec![s::TemplateSpec {
                template_id: "step.closed".into(),
                kind: "step.outcome".into(),
                parts: vec![s::TemplatePart::Literal("Read plan.".into())],
            }]
        }))
        .unwrap();
        super::super::root_update_tests::unbooted_with_inventory(inventory)
    } else {
        SummaryFixture::new(vec![])
    };
    let work = pinned();
    let mut plan = fx.snapshot();
    plan.plan = work.plan.clone();
    plan.status = s::PlanStatus::Locked;
    plan.lock_source = Some(plan.operator_source.clone());
    plan.categories = vec![s::Category {
        id: "work".into(),
        verbs: vec!["read".into()],
        acts: vec![s::PlanAct::Statement],
        review_required,
    }];
    let step = |id: u64, ordinal: u64, active: bool| s::PlanStep {
        step_id: s::StepId {
            seed_id: work.step_id.seed_id.clone(),
            local_id: id.into(),
        },
        ordinal: ordinal.into(),
        instruction: s::StatementRef {
            block_id: s::SourceBlockId {
                run: "run".into(),
                sequence: 1.into(),
                block: ordinal.into(),
            },
            binding: 0.into(),
        },
        category: "work".into(),
        group: None,
        status: if active {
            s::StepStatus::Active
        } else {
            s::StepStatus::Waiting
        },
        span: active.then(|| work.span.clone()),
        outcome_span: None,
        review_call: None,
        work: None,
        closure_source: None,
    };
    plan.steps = vec![step(1, 1, true), step(2, 2, false)];
    let boot = fx.boot(Some(plan));
    let run = fx.run();
    run.context.install_place(boot, &mut run.clock).unwrap();
    let certificate = run
        .context
        .take_template_certificate(&mut run.clock)
        .unwrap()
        .unwrap();
    run.context.complete_wait(PumpDeadline::Enter).unwrap();
    run.context
        .arm(
            PumpDeadline::Decision,
            AbsoluteBound::after(run.clock.now().unwrap(), 5_000_000_000).unwrap(),
            &mut run.clock,
        )
        .unwrap();
    (fx, certificate)
}

/// Serve exact bytes as consecutive pages of at most `capacity` octets,
/// then release the acquired snapshot.
fn serve(fx: &mut SummaryFixture, bytes: &[u8], capacity: usize) {
    let read = next(fx);
    assert!(matches!(
        &read,
        s::StateFrame::Control {
            frame: s::StateControl::Read {
                selector: s::ReadSelector::SpanPreview { .. }
            },
            ..
        }
    ));
    let mut offset = 0;
    let mut request = read;
    loop {
        let end = (offset + capacity).min(bytes.len());
        fx.boot
            .member
            .respond(&request, page(61, offset, bytes.len(), &bytes[offset..end]));
        offset = end;
        request = next(fx);
        if offset == bytes.len() {
            break;
        }
        assert!(
            matches!(&request, s::StateFrame::Control { frame: s::StateControl::Next { offset: at, .. }, .. } if at.0 as usize == offset)
        );
    }
    assert!(matches!(&request, s::StateFrame::Control {
        frame: s::StateControl::Release { snapshot_id }, .. } if snapshot_id.0 == 61));
    assert!(take(fx).is_none(), "an unreleased preview grants nothing");
    fx.boot.member.respond(
        &request,
        s::StateControlAnswer::Released {
            snapshot_id: 61.into(),
        },
    );
}

fn checked_work(fx: &mut SummaryFixture, capacity: usize) -> CheckedWork {
    begin(fx, work_request());
    serve(fx, PREVIEW, capacity);
    match finish(fx).into_checked_work() {
        Ok(work) => work,
        Err(_) => panic!("the pinned preview must construct checked work"),
    }
}

fn attest(fx: &mut SummaryFixture, work: CheckedWork) -> PlanPortCompletion {
    let reference = work.reference().clone();
    begin(
        fx,
        PlanRequest::AttestClosure {
            expected: reference.plan,
            step_id: reference.step_id,
            work,
        },
    );
    finish(fx)
}

fn evidence(fx: &mut SummaryFixture) -> ClosureEvidence {
    let work = checked_work(fx, PREVIEW.len());
    match attest(fx, work).into_closure_evidence() {
        Ok(evidence) => evidence,
        Err(_) => panic!("permitted unreviewed closure must attest"),
    }
}

/// The caller allocates outcome_span through the outcome_span operation: one
/// recorded Marks read, which the owner holds for the next close.
fn outcome_span_for(fx: &mut SummaryFixture, expected: s::PlanRef) -> s::SpanRef {
    begin(fx, PlanRequest::OutcomeSpan { expected });
    let read = next(fx);
    assert!(matches!(
        &read,
        s::StateFrame::Control {
            frame: s::StateControl::Read {
                selector: s::ReadSelector::Marks {}
            },
            ..
        }
    ));
    let bytes = serde_json::to_vec(&s::QueryResult::Marks {
        next_span_id: 4.into(),
        current_task: None,
        open_scopes: vec![],
    })
    .unwrap();
    fx.boot
        .member
        .respond(&read, page(62, 0, bytes.len(), &bytes));
    let release = next(fx);
    fx.boot.member.respond(
        &release,
        s::StateControlAnswer::Released {
            snapshot_id: 62.into(),
        },
    );
    let span = finish(fx)
        .outcome_span()
        .cloned()
        .expect("a recorded marks catalog allocates the outcome span");
    assert!(fx.run().context.outcome_marks.is_some());
    span
}

fn outcome_span(fx: &mut SummaryFixture) -> s::SpanRef {
    outcome_span_for(fx, pinned().plan)
}

fn begin_close(fx: &mut SummaryFixture, evidence: ClosureEvidence, outcome: s::SpanRef) {
    let work = pinned();
    let run = fx.run();
    let turn = TurnRef("closure".into());
    run.context
        .author_and_wait(
            &mut ports(&mut run.clock),
            AuthoredEvent {
                kind: Kind::TurnStarted,
                subsystem: weaver_trace::Subsystem::Harness,
                turn: Some(turn.clone()),
                source: AuthoredSource::Runtime,
                causal_parent: None,
                payload: None,
            },
        )
        .unwrap();
    run.context
        .begin_plan_operation(
            PlanRequest::Close {
                expected: work.plan,
                step_id: work.step_id,
                evidence,
                outcome_span: outcome,
            },
            Some(turn),
            &mut run.clock,
        )
        .unwrap();
}

fn effect_ready(fx: &mut SummaryFixture) {
    let ready = (0..128).any(|_| advance(fx).unwrap() == PlanPortProgress::EffectReady);
    assert!(ready, "close must prepare its effect after the reread");
}

fn refused_close(fx: &mut SummaryFixture) -> s::PlanOperationRefused {
    let PlanPortCompletion::Refused { refusal, .. } = finish(fx) else {
        panic!("close must refuse")
    };
    let rows = fx.records(Kind::Refusal);
    assert_eq!(rows.len(), 1);
    assert_eq!(rows[0]["payload"]["operation"], "close");
    assert_eq!(
        rows[0]["payload"]["refusal"],
        serde_json::to_value(&refusal).unwrap()
    );
    assert!(fx.records(Kind::StepOutcome).is_empty());
    assert!(fx.files.is_empty(), "a refused close files no effect");
    refusal
}

#[test]
fn closure_d1_page_capacity_sweep_yields_the_store_accepted_work_reference() {
    // Several page capacities, each smaller than the preview, and one page.
    // The fake trace sink bounds the page count, not the reassembly.
    for capacity in [7, 64, 1000, PREVIEW.len()] {
        let (mut fx, _) = closure_fixture(false, false);
        let work = checked_work(&mut fx, capacity);
        assert_eq!(work.reference(), &pinned(), "capacity {capacity}");
        assert_eq!(work.preview().raw_bytes(), PREVIEW);
    }
    // Equal JSON is not equal evidence: one changed byte changes the digest.
    let (mut fx, _) = closure_fixture(false, false);
    let mut changed = PREVIEW.to_vec();
    let at = changed.iter().position(|&b| b == b'z').unwrap();
    changed[at] = b'y';
    begin(&mut fx, work_request());
    serve(&mut fx, &changed, 64);
    let Ok(work) = finish(&mut fx).into_checked_work() else {
        panic!("a well-formed preview constructs checked work")
    };
    assert_ne!(work.reference().content_sha256, pinned().content_sha256);
    assert_eq!(
        work.reference().content_sha256.as_str(),
        format!("{:x}", Sha256::digest(&changed))
    );
    // An equal JSON value in another encoding is other evidence. The digest
    // is over the released bytes, never a parsed reconstruction.
    let (mut fx, _) = closure_fixture(false, false);
    let value: serde_json::Value = serde_json::from_slice(PREVIEW).unwrap();
    let pretty = serde_json::to_vec_pretty(&value).unwrap();
    assert_ne!(pretty, PREVIEW);
    begin(&mut fx, work_request());
    serve(&mut fx, &pretty, 64);
    let Ok(work) = finish(&mut fx).into_checked_work() else {
        panic!("an equal-valued preview constructs checked work")
    };
    assert_eq!(
        work.reference().content_sha256.as_str(),
        format!("{:x}", Sha256::digest(&pretty))
    );
    assert_ne!(work.reference().content_sha256, pinned().content_sha256);
}

#[test]
fn closure_attest_review_required_refuses_and_records_without_effect() {
    let (mut fx, _) = closure_fixture(true, false);
    let work = checked_work(&mut fx, PREVIEW.len());
    let preview_parent = fx.records(Kind::Recall).last().unwrap()["sequence"].clone();
    let PlanPortCompletion::Refused { refusal, .. } = attest(&mut fx, work) else {
        panic!("required review must refuse unreviewed closure")
    };
    assert_eq!(refusal.case, s::PlanOperationCase::ReviewRequired);
    assert_eq!(refusal.read_code, None);
    let rows = fx.records(Kind::Refusal);
    assert_eq!(rows.len(), 1);
    let work = pinned();
    assert_eq!(
        rows[0]["payload"],
        serde_json::to_value(weaver_types::RefusalRecord::PlanOperation {
            operation: s::PlanPortOperation::AttestClosure,
            expected: work.plan,
            step_id: Some(work.step_id),
            refusal,
        })
        .unwrap()
    );
    assert_eq!(rows[0]["causal_parent"], preview_parent);
    assert!(fx.records(Kind::PlanEvent).is_empty());
    assert!(fx.records(Kind::StepOutcome).is_empty());
    fx.boot.member.no_request();
}

#[test]
fn closure_attest_permitted_grants_evidence_without_read_or_effect() {
    let (mut fx, _) = closure_fixture(false, false);
    let work = checked_work(&mut fx, PREVIEW.len());
    let completion = attest(&mut fx, work);
    let evidence = completion.closure_evidence().expect("closure evidence");
    assert_eq!(evidence.work(), &pinned());
    assert_eq!(evidence.review_call(), None);
    fx.boot.member.no_request();
    assert!(fx.records(Kind::Refusal).is_empty());
    assert!(fx.records(Kind::StepOutcome).is_empty());
}

#[test]
fn closure_close_changed_work_bytes_refuse_work_changed_with_null_code() {
    let (mut fx, _) = closure_fixture(false, false);
    let evidence = evidence(&mut fx);
    let outcome = outcome_span(&mut fx);
    let before = fx.run().context.committed_plan_summary().unwrap().cloned();
    begin_close(&mut fx, evidence, outcome);
    // One encoded byte changed under the same echoed selector.
    let mut changed = PREVIEW.to_vec();
    let at = changed.iter().position(|&b| b == b'z').unwrap();
    changed[at] = b'y';
    serve(&mut fx, &changed, 500);
    let refusal = refused_close(&mut fx);
    assert_eq!(refusal.case, s::PlanOperationCase::WorkChanged);
    assert_eq!(refusal.read_code, None);
    // The parent is the reread's Recorded proof, the latest consumed.
    assert_eq!(
        fx.records(Kind::Refusal)[0]["causal_parent"],
        fx.records(Kind::Recall).last().unwrap()["sequence"]
    );
    assert_eq!(
        fx.run().context.committed_plan_summary().unwrap(),
        before.as_ref()
    );
}

#[test]
fn closure_close_incomplete_reread_keeps_its_read_code() {
    for code in [
        s::ReadRefusalCode::SpanPending,
        s::ReadRefusalCode::PartialPair,
    ] {
        let (mut fx, _) = closure_fixture(false, false);
        let evidence = evidence(&mut fx);
        let outcome = outcome_span(&mut fx);
        begin_close(&mut fx, evidence, outcome);
        let read = next(&mut fx);
        fx.boot.member.respond(&read, refuse(code));
        let refusal = refused_close(&mut fx);
        assert_eq!(refusal.case, s::PlanOperationCase::WorkChanged);
        assert_eq!(refusal.read_code, Some(code));
    }
}

#[test]
fn closure_close_stale_revision_refuses_before_any_read() {
    let (mut fx, _) = closure_fixture(false, false);
    let evidence = evidence(&mut fx);
    let outcome = outcome_span(&mut fx);
    let work = pinned();
    let run = fx.run();
    let turn = TurnRef("closure".into());
    run.context
        .author_and_wait(
            &mut ports(&mut run.clock),
            AuthoredEvent {
                kind: Kind::TurnStarted,
                subsystem: weaver_trace::Subsystem::Harness,
                turn: Some(turn.clone()),
                source: AuthoredSource::Runtime,
                causal_parent: None,
                payload: None,
            },
        )
        .unwrap();
    let mut stale = work.plan.clone();
    stale.revision = 2.into();
    run.context
        .begin_plan_operation(
            PlanRequest::Close {
                expected: stale,
                step_id: work.step_id,
                evidence,
                outcome_span: outcome,
            },
            Some(turn),
            &mut run.clock,
        )
        .unwrap();
    let refusal = refused_close(&mut fx);
    assert_eq!(refusal.case, s::PlanOperationCase::InvalidTarget);
    assert_eq!(refusal.read_code, None);
    fx.boot.member.no_request();
}

#[test]
fn closure_close_echo_violation_or_open_pair_is_a_member_protocol_fault() {
    for open_pair in [false, true] {
        let (mut fx, _) = closure_fixture(false, false);
        let evidence = evidence(&mut fx);
        let outcome = outcome_span(&mut fx);
        begin_close(&mut fx, evidence, outcome);
        let mut result: s::QueryResult = serde_json::from_slice(PREVIEW).unwrap();
        let s::QueryResult::SpanPreview {
            through, entries, ..
        } = &mut result
        else {
            unreachable!()
        };
        if open_pair {
            let pair = &mut entries[1].pairs[0];
            pair.fate = s::PairFate::Opened;
            pair.reason = None;
            pair.terminal_source = None;
        } else {
            through.block_id.sequence.0 += 1;
        }
        let bytes = serde_json::to_vec(&result).unwrap();
        let read = next(&mut fx);
        fx.boot
            .member
            .respond(&read, page(75, 0, bytes.len(), &bytes));
        let failure = (0..128)
            .find_map(|_| advance(&mut fx).err())
            .expect("an invalid successful preview is no ordinary refusal");
        assert!(matches!(
            failure,
            RunFailure::Flow {
                reason: FlowHaltReason::ProtocolFailed,
                ..
            }
        ));
        assert!(fx.records(Kind::Refusal).is_empty());
        assert!(fx.records(Kind::StepOutcome).is_empty());
    }
}

fn committed(fx: &mut SummaryFixture, cancel_after_source: bool) -> s::SourcePosition {
    let mut cancelled = false;
    for _ in 0..512 {
        advance(fx).unwrap();
        if cancel_after_source && !cancelled && !fx.records(Kind::StepOutcome).is_empty() {
            fx.run().context.cancel_plan_operation();
            cancelled = true;
        }
        assert!(take(fx).is_none(), "no result before the mandatory summary");
        let context = &mut fx.run().context;
        if context.filing.is_none()
            && let Some(required) = context.plan_cache.required.as_ref()
        {
            assert_eq!(cancelled, cancel_after_source);
            return required.source.clone();
        }
        fx.classifier(false);
        fx.file_reply();
    }
    panic!("admitted close did not retain its committed receipt");
}

fn closed_snapshot(
    fx: &mut SummaryFixture,
    source: &s::SourcePosition,
    outcome: &s::SpanRef,
) -> s::PlanSnapshot {
    let s::QueryResult::PlanCurrent {
        current: Some(snapshot),
        ..
    } = fx
        .run()
        .context
        .plan_cache
        .committed
        .as_ref()
        .unwrap()
        .read()
        .result()
    else {
        panic!()
    };
    let mut snapshot = snapshot.clone();
    let step = &mut snapshot.steps[0];
    step.status = s::StepStatus::Closed;
    step.outcome_span = Some(outcome.clone());
    step.work = Some(pinned());
    step.closure_source = Some(source.block_id.clone());
    snapshot.last_source = source.clone();
    snapshot
}

fn release_summary(fx: &mut SummaryFixture, snapshot: s::PlanSnapshot) {
    let read = next(fx);
    assert!(matches!(
        &read,
        s::StateFrame::Control {
            frame: s::StateControl::Read {
                selector: s::ReadSelector::PlanCurrent {}
            },
            ..
        }
    ));
    let bytes = summary_bytes(Some(snapshot));
    fx.boot
        .member
        .respond(&read, page(91, 0, bytes.len(), &bytes));
    let release = next(fx);
    fx.boot.member.respond(
        &release,
        s::StateControlAnswer::Released {
            snapshot_id: 91.into(),
        },
    );
}

#[test]
fn closure_close_files_one_exact_step_outcome_and_adopts_the_snapshot() {
    for cancel_at in ["none", "source", "receipt"] {
        let (mut fx, certificate) = closure_fixture(false, true);
        let bound = fx.run().context.deadlines.get(PumpDeadline::Decision);
        let evidence = evidence(&mut fx);
        let outcome = outcome_span(&mut fx);
        assert_eq!(outcome.local_id.0, 4);
        begin_close(&mut fx, evidence, outcome.clone());
        // The reread pages the same bytes at a different capacity.
        serve(&mut fx, PREVIEW, 333);
        effect_ready(&mut fx);
        let run = fx.run();
        run.context
            .begin_plan_effect(&certificate, &mut run.clock)
            .unwrap();
        let source = committed(&mut fx, cancel_at == "source");
        if cancel_at == "receipt" {
            fx.run().context.cancel_plan_operation();
        }
        let snapshot = closed_snapshot(&mut fx, &source, &outcome);
        release_summary(&mut fx, snapshot.clone());
        let result = finish(&mut fx);
        if cancel_at == "none" {
            assert_eq!(result.closed_snapshot(), Some(&snapshot));
        } else {
            assert!(matches!(result, PlanPortCompletion::Cancelled(_)));
        }
        // An admitted effect settles its refresh and adoption even when cancelled.
        assert!(fx.run().context.native_place_cache_ready(), "{cancel_at}");
        assert_eq!(
            fx.run()
                .context
                .committed_plan_summary()
                .unwrap()
                .unwrap()
                .last_source,
            source
        );
        assert!(fx.run().context.plan_port_work().is_none());
        assert_eq!(
            fx.run().context.deadlines.get(PumpDeadline::Decision),
            bound
        );

        let effect = s::PlanEffect::Close {
            expected: pinned().plan,
            step_id: pinned().step_id,
            review_call: None,
            work: pinned(),
            outcome_span: outcome.clone(),
        };
        let outcomes = fx.records(Kind::StepOutcome);
        assert_eq!(outcomes.len(), 1);
        let payload = &outcomes[0]["payload"];
        assert_eq!(payload["context"], serde_json::to_value(&effect).unwrap());
        assert_eq!(payload["template_id"], "step.closed");
        assert_eq!(payload["outcome"], serde_json::json!({"closed": {}}));
        assert_eq!(payload["review_call"], serde_json::Value::Null);
        assert_eq!(payload["work"], serde_json::to_value(pinned()).unwrap());
        assert_eq!(
            payload["message"],
            serde_json::json!({"role":"user","content":[{"type":"text","text":"Read plan."}]})
        );
        assert_eq!(outcomes[0]["channel"], "internal");
        assert_eq!(outcomes[0]["origin"], "loop");
        assert_eq!(outcomes[0]["turn"], "closure");
        assert!(fx.records(Kind::PlanEvent).is_empty());
        assert!(fx.records(Kind::Refusal).is_empty());
        assert!(fx.records(Kind::ModelRequest).is_empty());
        assert_eq!(fx.files.len(), 1);
        assert_eq!(fx.files[0].source.kind, "step.outcome");
        assert_eq!(
            fx.files[0].effect,
            Some(s::StatementEffect::Plan(Box::new(effect)))
        );
    }
}

#[test]
fn closure_close_refresh_without_the_exact_closure_latches() {
    let (mut fx, certificate) = closure_fixture(false, true);
    let evidence = evidence(&mut fx);
    let outcome = outcome_span(&mut fx);
    begin_close(&mut fx, evidence, outcome.clone());
    serve(&mut fx, PREVIEW, PREVIEW.len());
    effect_ready(&mut fx);
    let run = fx.run();
    run.context
        .begin_plan_effect(&certificate, &mut run.clock)
        .unwrap();
    let source = committed(&mut fx, false);
    let mut snapshot = closed_snapshot(&mut fx, &source, &outcome);
    snapshot.steps[0].outcome_span = None;
    release_summary(&mut fx, snapshot);
    let failure = (0..128)
        .find_map(|_| advance(&mut fx).err())
        .expect("a refresh without this closure must fail");
    assert!(format!("{failure:?}").contains("close refresh changed its committed closure"));
    assert!(fx.records(Kind::Refusal).is_empty());
}

#[test]
fn closure_close_foreign_evidence_latches_without_read_or_effect() {
    let (mut original, _) = closure_fixture(false, false);
    let (mut other, _) = closure_fixture(false, false);
    assert_eq!(
        original.seed(),
        other.seed(),
        "public identities deliberately match"
    );
    let foreign = evidence(&mut other);
    let outcome = outcome_span(&mut original);
    begin_close(&mut original, foreign, outcome);
    let failure = advance(&mut original).unwrap_err();
    assert!(format!("{failure:?}").contains("foreign closure evidence"));
    original.boot.member.no_request();
    assert!(original.records(Kind::Refusal).is_empty());
    assert!(original.records(Kind::StepOutcome).is_empty());
}

#[test]
fn closure_close_outcome_span_outside_its_held_marks_refuses_before_any_read() {
    for field in ["revision", "seed"] {
        let (mut fx, _) = closure_fixture(false, false);
        let evidence = evidence(&mut fx);
        let held = outcome_span(&mut fx);
        assert_eq!((held.local_id.0, held.revision.0), (4, 1));
        assert_eq!(
            held.seed_id,
            fx.run().context.open_start.as_ref().unwrap().seed_id
        );
        let mut outcome = held.clone();
        if field == "revision" {
            outcome.revision = 2.into();
        } else {
            outcome.seed_id.push('x');
        }
        let before = fx.run().context.committed_plan_summary().unwrap().cloned();
        begin_close(&mut fx, evidence, outcome);
        let refusal = refused_close(&mut fx);
        assert_eq!(refusal.case, s::PlanOperationCase::InvalidTarget, "{field}");
        assert_eq!(refusal.read_code, None, "{field}");
        fx.boot.member.no_request();
        assert_eq!(
            fx.run().context.committed_plan_summary().unwrap(),
            before.as_ref()
        );
    }
}

/// Answer the next statement filing with the store's guard refusal.
fn refuse_filing(fx: &mut SummaryFixture, detail: &str) -> bool {
    fx.transport(TransportSource::State);
    let mut bytes = [0; 65536];
    let n = match fx.boot.member.run.state.read(&mut bytes) {
        Ok(0) => panic!("filing member closed"),
        Ok(n) => n,
        Err(error) if error.kind() == io::ErrorKind::WouldBlock => return false,
        Err(error) => panic!("filing member: {error}"),
    };
    let request = s::decode_state_line(&bytes[..n], s::FrameDirection::Request).unwrap();
    let s::StateFrame::File { filing, .. } = &request else {
        panic!("expected the close statement filing");
    };
    fx.files.push(filing.clone());
    let began = fx.boot.member.now() + 1;
    let observation = operation(&request, fx.boot.member.next_operation, began);
    fx.boot.member.next_operation += 1;
    fx.boot
        .member
        .receive(&s::StateFrame::Refused {
            common: request.common().clone(),
            code: s::StateRefusalCode::InvalidEffect,
            detail: detail.into(),
            block_id: Some(filing.block_id.clone()),
            observation,
        })
        .unwrap();
    true
}

#[test]
fn closure_close_store_guard_refusal_halts_without_closure_or_refresh() {
    let (mut fx, certificate) = closure_fixture(false, true);
    let evidence = evidence(&mut fx);
    // A span allocated after the held read: the local check admits it, and
    // the store's consecutive-span guard refuses the filed effect.
    let mut raced = outcome_span(&mut fx);
    raced.local_id = (raced.local_id.0 + 1).into();
    let before = fx.run().context.committed_plan_summary().unwrap().cloned();
    begin_close(&mut fx, evidence, raced.clone());
    serve(&mut fx, PREVIEW, PREVIEW.len());
    effect_ready(&mut fx);
    let run = fx.run();
    run.context
        .begin_plan_effect(&certificate, &mut run.clock)
        .unwrap();
    let mut refused = false;
    let mut failure = None;
    for _ in 0..512 {
        match advance(&mut fx) {
            Err(error) => {
                failure = Some(error);
                break;
            }
            Ok(_) => assert!(take(&mut fx).is_none(), "a refused filing grants nothing"),
        }
        fx.classifier(false);
        refused |= refuse_filing(&mut fx, "span allocation is not consecutive");
    }
    assert!(refused, "the close effect reached the store");
    let failure = failure.expect("a refused statement filing halts the run");
    assert!(
        matches!(
            &failure,
            RunFailure::Flow {
                reason: FlowHaltReason::FilingRefused,
                detail,
                ..
            } if detail.contains("span allocation is not consecutive")
        ),
        "{failure:?}"
    );
    // The effect was the exact close filing, and nothing settled after it:
    // no operation refusal, no refresh, and the cache still holds the step.
    assert_eq!(fx.files.len(), 1);
    assert!(matches!(
        &fx.files[0].effect,
        Some(s::StatementEffect::Plan(effect))
            if matches!(&**effect, s::PlanEffect::Close { outcome_span, .. } if outcome_span == &raced)
    ));
    assert!(fx.records(Kind::Refusal).is_empty());
    assert!(fx.run().context.plan_cache.required.is_none());
    let s::QueryResult::PlanCurrent {
        current: Some(plan),
        ..
    } = fx
        .run()
        .context
        .plan_cache
        .committed
        .as_ref()
        .unwrap()
        .read()
        .result()
    else {
        panic!()
    };
    assert_eq!(plan.steps[0].status, s::StepStatus::Active);
    assert_eq!(plan.steps[0].outcome_span, None);
    assert_eq!(plan.plan, before.unwrap().plan);
}

/// A plain recorded preview of the held selector, read by the test.
fn recorded_preview(fx: &mut SummaryFixture) -> RecordedRead {
    let work = pinned();
    let run = fx.run();
    run.context
        .begin_read(
            s::ReadSelector::SpanPreview {
                span: work.span,
                through: work.through,
            },
            None,
            &mut run.clock,
        )
        .unwrap();
    let mut answered = 0;
    for _ in 0..256 {
        let run = fx.run();
        run.context
            .advance_read(&mut ports(&mut run.clock))
            .unwrap();
        if let Some(done) = run.context.take_read(&mut run.clock).unwrap() {
            let ReadCompletion::Recorded(read) = done else {
                panic!("test preview refused")
            };
            return read;
        }
        if !fx.run().context.flow.state().outstanding().is_empty() {
            let request = fx.boot.member.sent();
            let answer = if answered == 0 {
                page(63, 0, PREVIEW.len(), PREVIEW)
            } else {
                s::StateControlAnswer::Released {
                    snapshot_id: 63.into(),
                }
            };
            fx.boot.member.respond(&request, answer);
            answered += 1;
        }
    }
    panic!("test preview did not settle")
}

#[test]
fn closure_close_reread_must_follow_the_attestation() {
    let (mut fx, _) = closure_fixture(false, false);
    // Marks first, then an exact preview older than the attested work: it
    // follows the cache and the marks, and precedes the attestation.
    let outcome = outcome_span(&mut fx);
    let stale = recorded_preview(&mut fx);
    let evidence = evidence(&mut fx);
    let held = fx.run().context.outcome_marks.as_ref().unwrap().sequence();
    assert!(stale.recall_sequence().0 > held.0);
    assert!(stale.recall_sequence().0 < evidence.sequence().0);
    let work = pinned();
    let request = PlanRequest::Close {
        expected: work.plan,
        step_id: work.step_id,
        evidence,
        outcome_span: outcome,
    };
    let run = fx.run();
    let current = std::sync::Arc::clone(run.context.plan_cache.committed.as_ref().unwrap());
    assert!(stale.recall_sequence().0 > current.read().recall_sequence().0);
    let bound = run.context.deadlines.get(PumpDeadline::Decision).unwrap();
    let result = crate::flow::plan::ports::closure::prepare_close(
        &request,
        run.context.outcome_marks.as_ref(),
        &run.context,
        &current,
        &stale,
        TurnRef("closure".into()),
        bound,
    );
    let Err(crate::flow::plan::ports::PlanCheckFailure::Protocol(detail)) = result else {
        panic!("a reread older than the attestation must not prepare an effect")
    };
    assert_eq!(detail, "close lost its ordered work reread or turn");
}

fn begin_close_unadmitted(fx: &mut SummaryFixture, evidence: ClosureEvidence) {
    let work = pinned();
    let run = fx.run();
    // close requires a turn: refused at admission, nothing is consumed.
    let Err(rejected) = run.context.begin_plan_operation(
        PlanRequest::Close {
            expected: work.plan,
            step_id: work.step_id,
            evidence,
            outcome_span: s::SpanRef {
                seed_id: String::new(),
                local_id: 0.into(),
                revision: 0.into(),
            },
        },
        None,
        &mut run.clock,
    ) else {
        panic!("close without a turn is not admitted")
    };
    assert_eq!(rejected.refusal, PlanPortAdmission::InvalidAttribution);
}

#[test]
fn closure_close_without_a_held_outcome_read_refuses_before_any_read() {
    for case in ["none", "consumed", "other-revision", "replaced-refused"] {
        let (mut fx, _) = closure_fixture(false, false);
        let outcome = match case {
            "none" => pinned().span,
            "other-revision" => {
                let mut other = pinned().plan;
                other.revision = 2.into();
                outcome_span_for(&mut fx, other)
            }
            _ => outcome_span(&mut fx),
        };
        if case == "consumed" {
            // An admitted close takes the held read even when it refuses.
            let evidence = evidence(&mut fx);
            let mut wrong = outcome.clone();
            wrong.revision = 2.into();
            begin_close(&mut fx, evidence, wrong);
            let refusal = refused_close(&mut fx);
            assert_eq!(refusal.case, s::PlanOperationCase::InvalidTarget);
            assert!(fx.run().context.outcome_marks.is_none());
        }
        if case == "replaced-refused" {
            // A new outcome_span replaces the held read at admission; its
            // refused read leaves none behind.
            begin(
                &mut fx,
                PlanRequest::OutcomeSpan {
                    expected: pinned().plan,
                },
            );
            assert!(fx.run().context.outcome_marks.is_none());
            let read = next(&mut fx);
            fx.boot
                .member
                .respond(&read, refuse(s::ReadRefusalCode::CounterExhausted));
            assert!(matches!(
                finish(&mut fx),
                PlanPortCompletion::QueryRefused(_)
            ));
        }
        let evidence = evidence(&mut fx);
        let prior = fx.records(Kind::Refusal).len();
        let before = fx.run().context.committed_plan_summary().unwrap().cloned();
        begin_close(&mut fx, evidence, outcome);
        let PlanPortCompletion::Refused { refusal, .. } = finish(&mut fx) else {
            panic!("{case}: close without a held read must refuse")
        };
        assert_eq!(refusal.case, s::PlanOperationCase::InvalidTarget, "{case}");
        assert_eq!(refusal.read_code, None, "{case}");
        assert_eq!(
            refusal.detail,
            "close has no held outcome_span read for this run and plan revision"
        );
        let rows = fx.records(Kind::Refusal);
        assert_eq!(rows.len(), prior + 1, "{case}");
        assert_eq!(rows[prior]["payload"]["operation"], "close");
        assert!(fx.records(Kind::StepOutcome).is_empty());
        fx.boot.member.no_request();
        assert_eq!(
            fx.run().context.committed_plan_summary().unwrap(),
            before.as_ref()
        );
    }
}

#[test]
fn closure_close_refused_at_admission_keeps_the_held_read() {
    let (mut fx, _) = closure_fixture(false, false);
    let outcome = outcome_span(&mut fx);
    let held = fx.run().context.outcome_marks.as_ref().unwrap().sequence();
    let first = evidence(&mut fx);
    begin_close_unadmitted(&mut fx, first);
    assert_eq!(
        fx.run()
            .context
            .outcome_marks
            .as_ref()
            .map(|held| held.sequence()),
        Some(held)
    );
    // The unadmitted request returned whole; a fresh grant closes with the
    // same held read through the reread.
    let evidence = evidence(&mut fx);
    begin_close(&mut fx, evidence, outcome);
    assert!(fx.run().context.outcome_marks.is_none());
    serve(&mut fx, PREVIEW, PREVIEW.len());
    effect_ready(&mut fx);
}
