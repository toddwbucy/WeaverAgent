//! Break consumes actual cached/read/filing proofs and records before return.
use super::*;
use crate::flow::templates::{CompiledInventory, TemplateCertificate};

fn break_request(fx: &SummaryFixture, active: bool) -> PlanRequest {
    let req = request(fx.seed());
    PlanRequest::RecordBreak {
        expected: req.expected().clone(),
        step_id: active.then(|| req.step_id().unwrap().clone()),
        case: s::PlanBreakCase::WorkChanged,
        proposed_source: Some(s::SourceBlockId {
            run: "proposed-run".into(),
            sequence: 12.into(),
            block: 3.into(),
        }),
    }
}
fn begin_turn(fx: &mut SummaryFixture, request: PlanRequest) {
    let run = fx.run();
    let turn = TurnRef("break".into());
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
        .begin_plan_operation(request, Some(turn), &mut run.clock)
        .unwrap();
}
fn certified(active: bool) -> (SummaryFixture, TemplateCertificate) {
    static SPECS: std::sync::OnceLock<Vec<s::TemplateSpec>> = std::sync::OnceLock::new();
    let inventory = CompiledInventory::new(SPECS.get_or_init(|| {
        vec![s::TemplateSpec {
            template_id: "plan.break".into(),
            kind: "plan.event".into(),
            parts: vec![s::TemplatePart::Literal("Read plan.".into())],
        }]
    }))
    .unwrap();
    let mut fx = super::super::root_update_tests::unbooted_with_inventory(inventory);
    let run = fx.run();
    let certificate = run
        .context
        .take_template_certificate(&mut run.clock)
        .unwrap()
        .unwrap();
    let fx = ready_configured_with_place(
        fx,
        true,
        false,
        if active {
            s::StepStatus::Active
        } else {
            s::StepStatus::Waiting
        },
        s::Category {
            id: "work".into(),
            verbs: vec!["read".into()],
            acts: vec![s::PlanAct::Statement],
            review_required: false,
        },
        true,
    );
    (fx, certificate)
}
fn start_effect(fx: &mut SummaryFixture, certificate: &TemplateCertificate) {
    assert!((0..128).any(|_| advance(fx).unwrap() == PlanPortProgress::EffectReady));
    let run = fx.run();
    run.context
        .begin_plan_effect(certificate, &mut run.clock)
        .unwrap();
}
fn committed(fx: &mut SummaryFixture, cancel_after_source: bool) -> s::SourcePosition {
    let bound = fx.run().context.plan_port_bound();
    let mut cancelled = false;
    for _ in 0..512 {
        advance(fx).unwrap();
        if cancel_after_source && !cancelled && !fx.records(Kind::PlanEvent).is_empty() {
            assert!(fx.files.is_empty());
            fx.run().context.cancel_plan_operation();
            cancelled = true;
        }
        assert!(take(fx).is_none());
        assert!(fx.records(Kind::PlanBreak).is_empty());
        assert_eq!(fx.run().context.plan_port_bound(), bound);
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
    panic!("break effect did not retain its committed receipt");
}
fn snapshot(fx: &mut SummaryFixture, source: s::SourcePosition) -> s::PlanSnapshot {
    let context = &fx.run().context;
    let s::QueryResult::PlanCurrent {
        current: Some(snapshot),
        ..
    } = context
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
    let PlanRequest::RecordBreak {
        step_id,
        case,
        proposed_source,
        ..
    } = context.plan_port_work().unwrap().test_request()
    else {
        panic!()
    };
    snapshot.status = s::PlanStatus::Broken;
    snapshot.broken = Some(s::PlanBreakRecord {
        case: *case,
        step_id: step_id.clone(),
        proposed_source: proposed_source.clone(),
        source: source.block_id.clone(),
    });
    snapshot.last_source = source;
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
        .respond(&read, page(95, 0, bytes.len(), &bytes));
    let release = next(fx);
    assert!(
        matches!(&release, s::StateFrame::Control { frame: s::StateControl::Release { snapshot_id }, .. } if snapshot_id.0 == 95)
    );
    assert!(take(fx).is_none());
    assert!(fx.records(Kind::PlanBreak).is_empty());
    fx.boot.member.respond(
        &release,
        s::StateControlAnswer::Released {
            snapshot_id: 95.into(),
        },
    );
}

#[test]
fn plan_break_without_locked_plan_records_original_metadata_without_effect_or_read() {
    let mut fx = ready(false);
    let req = break_request(&fx, true);
    let parent = fx
        .run()
        .context
        .plan_cache
        .committed
        .as_ref()
        .unwrap()
        .read()
        .recall_sequence();
    let bound = fx.run().context.deadlines.get(PumpDeadline::Decision);
    let expected = match &req {
        PlanRequest::RecordBreak {
            step_id,
            case,
            proposed_source,
            ..
        } => serde_json::to_value(s::PlanBreakReport {
            plan: None,
            step_id: step_id.clone(),
            case: *case,
            proposed_source: proposed_source.clone(),
        })
        .unwrap(),
        _ => unreachable!(),
    };
    begin(&mut fx, req);
    let result = finish(&mut fx);
    assert!(matches!(result, PlanPortCompletion::BreakRecorded(_)));
    assert!(result.current_snapshot().is_none());
    let rows = fx.records(Kind::PlanBreak);
    assert_eq!(rows.len(), 1);
    assert_eq!(rows[0]["payload"], expected);
    assert_eq!(rows[0]["causal_parent"], serde_json::json!(parent));
    assert!(rows[0].get("turn").is_none());
    assert!(fx.records(Kind::PlanEvent).is_empty());
    assert!(fx.records(Kind::Refusal).is_empty());
    assert_eq!(
        fx.run().context.deadlines.get(PumpDeadline::Decision),
        bound
    );
    fx.boot.member.no_request();
}

#[test]
fn plan_break_requires_exact_active_step_and_current_revision_before_filing() {
    for wrong in ["step", "absent", "revision"] {
        let mut fx = ready(true);
        let mut req = break_request(&fx, true);
        let PlanRequest::RecordBreak {
            expected, step_id, ..
        } = &mut req
        else {
            unreachable!()
        };
        match wrong {
            "step" => step_id.as_mut().unwrap().local_id = 99.into(),
            "absent" => *step_id = None,
            _ => expected.revision = 2.into(),
        }
        begin_turn(&mut fx, req);
        assert!(
            matches!(finish(&mut fx), PlanPortCompletion::Refused { refusal, .. } if refusal.case == s::PlanOperationCase::InvalidTarget)
        );
        assert!(fx.records(Kind::PlanEvent).is_empty());
        assert!(fx.records(Kind::PlanBreak).is_empty());
        assert_eq!(fx.records(Kind::Refusal).len(), 1);
        fx.boot.member.no_request();
    }
}

#[test]
fn plan_break_locked_without_turn_returns_original_admission_custody() {
    let mut fx = ready(true);
    let req = break_request(&fx, true);
    let run = fx.run();
    let error = run
        .context
        .begin_plan_operation(req, None, &mut run.clock)
        .unwrap_err();
    assert_eq!(error.refusal, PlanPortAdmission::InvalidAttribution);
    assert!(error.failure.is_none());
    assert!(matches!(
        error.request,
        PlanRequest::RecordBreak {
            case: s::PlanBreakCase::WorkChanged,
            ..
        }
    ));
    assert!(run.context.plan_port_work().is_none());
    assert!(fx.records(Kind::PlanBreak).is_empty());
}

#[test]
fn plan_break_committed_effect_refreshes_and_reports_even_after_cancellation() {
    for active in [false, true] {
        for cancel_at in ["none", "source", "receipt", "summary"] {
            let (mut fx, certificate) = certified(active);
            let req = break_request(&fx, active);
            begin_turn(&mut fx, req);
            start_effect(&mut fx, &certificate);
            let source = committed(&mut fx, cancel_at == "source");
            let snapshot = snapshot(&mut fx, source.clone());
            let bound = fx.run().context.plan_port_bound();
            if cancel_at == "receipt" {
                fx.run().context.cancel_plan_operation();
            }
            release_summary(&mut fx, snapshot.clone());
            if cancel_at == "summary" {
                fx.run().context.cancel_plan_operation();
            }
            let result = finish(&mut fx);
            if cancel_at == "none" {
                assert!(matches!(result, PlanPortCompletion::BreakRecorded(_)));
                assert_eq!(result.current_snapshot(), Some(&snapshot));
            } else {
                assert!(matches!(result, PlanPortCompletion::Cancelled(_)));
            }
            let rows = fx.records(Kind::PlanBreak);
            assert_eq!(rows.len(), 1);
            assert_eq!(
                rows[0]["causal_parent"],
                serde_json::json!(source.block_id.sequence)
            );
            assert_eq!(rows[0]["payload"]["case"], "work_changed");
            assert_eq!(
                rows[0]["payload"]["plan"],
                serde_json::to_value(&snapshot.plan).unwrap()
            );
            assert_eq!(fx.files.len(), 1);
            assert_eq!(fx.records(Kind::PlanEvent).len(), 1);
            assert!(fx.records(Kind::Refusal).is_empty());
            assert!(fx.run().context.plan_cache.required.is_none());
            assert!(
                fx.run().context.native_place_cache_ready(),
                "break must adopt its committed summary even after cancellation"
            );
            assert_eq!(
                fx.run().context.deadlines.get(PumpDeadline::Decision),
                bound
            );
        }
    }
}

#[test]
fn plan_break_cancel_before_source_preserves_plan_without_new_effect_or_report() {
    let (mut fx, certificate) = certified(true);
    let before = fx.run().context.committed_plan_summary().unwrap().cloned();
    let req = break_request(&fx, true);
    begin_turn(&mut fx, req);
    assert!((0..128).any(|_| advance(&mut fx).unwrap() == PlanPortProgress::EffectReady));
    fx.run().context.cancel_plan_operation();
    let run = fx.run();
    run.context
        .begin_plan_effect(&certificate, &mut run.clock)
        .unwrap();
    assert!(matches!(finish(&mut fx), PlanPortCompletion::Cancelled(_)));
    assert!(fx.records(Kind::PlanEvent).is_empty());
    assert!(fx.records(Kind::PlanBreak).is_empty());
    assert_eq!(
        fx.run().context.committed_plan_summary().unwrap(),
        before.as_ref()
    );
}

#[test]
fn plan_break_template_rejection_records_fault_then_refusal_without_break() {
    for cancelled in [false, true] {
        let (mut fx, certificate) = certified(true);
        let req = break_request(&fx, true);
        begin_turn(&mut fx, req);
        start_effect(&mut fx, &certificate);
        let mut cancelled_once = false;
        let result = (0..512)
            .find_map(|_| {
                advance(&mut fx).unwrap();
                if cancelled && !cancelled_once && !fx.records(Kind::PlanEvent).is_empty() {
                    fx.run().context.cancel_plan_operation();
                    cancelled_once = true;
                }
                fx.classifier(true);
                fx.file_reply();
                take(&mut fx)
            })
            .unwrap();
        if cancelled {
            assert!(matches!(result, PlanPortCompletion::Cancelled(_)));
        } else {
            assert!(
                matches!(result, PlanPortCompletion::Refused { refusal, .. } if refusal.case == s::PlanOperationCase::TemplateRejected)
            );
        }
        assert!(fx.files.is_empty());
        assert!(fx.records(Kind::PlanBreak).is_empty());
        let faults = fx.records(Kind::Fault);
        let refusals = fx.records(Kind::Refusal);
        assert_eq!((faults.len(), refusals.len()), (1, 1));
        assert_eq!(refusals[0]["causal_parent"], faults[0]["sequence"]);
        assert!(fx.run().context.failure().is_none());
    }
}

#[test]
fn plan_break_post_commit_summary_refusal_retains_receipt_and_withholds_report() {
    let (mut fx, certificate) = certified(true);
    let req = break_request(&fx, true);
    begin_turn(&mut fx, req);
    start_effect(&mut fx, &certificate);
    let source = committed(&mut fx, false);
    let read = next(&mut fx);
    fx.boot
        .member
        .respond(&read, refuse(s::ReadRefusalCode::ResultUnavailable));
    let failure = (0..128)
        .find_map(|_| advance(&mut fx).err())
        .expect("post-commit failure");
    assert!(format!("{failure:?}").contains("required post-effect summary read refused"));
    assert_eq!(
        fx.run()
            .context
            .plan_cache
            .required
            .as_ref()
            .unwrap()
            .source,
        source
    );
    assert!(fx.run().context.plan_port_work().is_some());
    assert!(fx.records(Kind::PlanBreak).is_empty());
    assert!(fx.records(Kind::Refusal).is_empty());
}

#[test]
fn plan_break_changed_summary_cannot_record_requested_metadata_as_committed() {
    for field in ["case", "proposal"] {
        let (mut fx, certificate) = certified(true);
        let req = break_request(&fx, true);
        begin_turn(&mut fx, req);
        start_effect(&mut fx, &certificate);
        let source = committed(&mut fx, false);
        let mut changed = snapshot(&mut fx, source);
        let broken = changed.broken.as_mut().unwrap();
        if field == "case" {
            broken.case = s::PlanBreakCase::SelfEdit;
        } else {
            broken.proposed_source = None;
        }
        release_summary(&mut fx, changed);
        let failure = (0..128).find_map(|_| advance(&mut fx).err()).unwrap();
        assert!(format!("{failure:?}").contains("break refresh changed its committed break"));
        assert!(fx.records(Kind::PlanBreak).is_empty());
        assert!(fx.run().context.plan_port_work().is_some());
        assert!(!fx.run().context.native_place_cache_ready());
    }
}

#[test]
fn plan_break_authorized_plan_records_only_report_and_preserves_actual_snapshot() {
    let mut fx = SummaryFixture::new(vec![]);
    let before = fx.snapshot();
    let _boot = fx.boot(Some(before.clone()));
    let run = fx.run();
    let before_summary = run.context.committed_plan_summary().unwrap().cloned();
    run.context.complete_wait(PumpDeadline::Enter).unwrap();
    let bound = AbsoluteBound::after(run.clock.now().unwrap(), 5_000_000_000).unwrap();
    run.context
        .arm(PumpDeadline::Decision, bound, &mut run.clock)
        .unwrap();
    let req = break_request(&fx, false);
    begin(&mut fx, req);
    let result = finish(&mut fx);
    assert!(matches!(result, PlanPortCompletion::BreakRecorded(_)));
    assert_eq!(result.current_snapshot(), Some(&before));
    assert_eq!(
        fx.run().context.committed_plan_summary().unwrap(),
        before_summary.as_ref()
    );
    let rows = fx.records(Kind::PlanBreak);
    assert_eq!(rows.len(), 1);
    assert!(rows[0]["payload"]["plan"].is_null());
    assert!(rows[0]["payload"]["step_id"].is_null());
    assert!(fx.records(Kind::PlanEvent).is_empty());
    fx.boot.member.no_request();
}

#[test]
fn plan_break_report_retains_exact_ticket_through_pressure_cancellation_and_expiry() {
    for committed_effect in [false, true] {
        for disposition in ["complete", "cancel", "expire"] {
            let mut fx = if committed_effect {
                let (mut fx, certificate) = certified(true);
                let req = break_request(&fx, true);
                begin_turn(&mut fx, req);
                start_effect(&mut fx, &certificate);
                let source = committed(&mut fx, false);
                let snapshot = snapshot(&mut fx, source);
                release_summary(&mut fx, snapshot);
                fx
            } else {
                let mut fx = ready(false);
                let req = break_request(&fx, false);
                begin(&mut fx, req);
                fx
            };
            assert!((0..128).any(|_| {
                advance(&mut fx).unwrap();
                fx.run()
                    .context
                    .plan_port_work()
                    .unwrap()
                    .test_stage()
                    .starts_with("BreakReport")
            }));
            assert!(fx.records(Kind::PlanBreak).is_empty());
            fx.boot.member.fill_trace();
            let run = fx.run();
            let original = run.context.plan_port_bound().unwrap();
            let label = AbsoluteBound::after(run.clock.now().unwrap(), 100).unwrap();
            run.context
                .arm(PumpDeadline::Label, label, &mut run.clock)
                .unwrap();
            assert_eq!(
                advance(&mut fx),
                Err(RunFailure::Wait {
                    key: PumpDeadline::Label,
                    bound: label
                })
            );
            let run = fx.run();
            run.context.complete_wait(PumpDeadline::Label).unwrap();
            let pending = run.context.pending_author().unwrap();
            let ticket = pending.ticket;
            let event = format!("{:?}", pending.event);
            assert_eq!(
                run.context
                    .plan_port_work()
                    .unwrap()
                    .test_refusal_account()
                    .1,
                Some(ticket)
            );
            assert!(
                run.context
                    .take_plan_operation(&mut run.clock)
                    .unwrap()
                    .is_none()
            );
            assert_eq!(
                run.context.deadlines.get(PumpDeadline::Decision),
                Some(original)
            );
            if disposition == "expire" {
                run.clock.advance_to(original.deadline_ns()).unwrap();
                assert!(matches!(
                    advance(&mut fx),
                    Err(RunFailure::Flow {
                        reason: FlowHaltReason::DecisionBound,
                        ..
                    })
                ));
                let run = fx.run();
                assert!(run.context.take_plan_operation(&mut run.clock).is_err());
                let pending = run.context.pending_author().unwrap();
                assert_eq!(pending.ticket, ticket);
                assert_eq!(format!("{:?}", pending.event), event);
                assert!(run.context.plan_port_work().is_some());
                assert!(fx.records(Kind::PlanBreak).is_empty());
            } else {
                if disposition == "cancel" {
                    run.context.cancel_plan_operation();
                }
                fx.drain();
                let result = finish(&mut fx);
                assert_eq!(
                    matches!(result, PlanPortCompletion::Cancelled(_)),
                    disposition == "cancel"
                );
                let rows = fx.records(Kind::PlanBreak);
                assert_eq!(rows.len(), 1);
                let sequence = weaver_trace::Sequence(
                    rows[0]["sequence"]
                        .as_str()
                        .unwrap()
                        .parse::<u64>()
                        .unwrap(),
                );
                assert_eq!(
                    fx.run().context.barrier.recorded_author_ticket(sequence),
                    Some(ticket)
                );
                assert_eq!(fx.files.len(), usize::from(committed_effect));
            }
        }
    }
}

#[test]
fn plan_break_report_waits_for_an_earlier_author_before_recording_and_completion() {
    for committed_effect in [false, true] {
        let mut fx = if committed_effect {
            let (mut fx, certificate) = certified(true);
            let req = break_request(&fx, true);
            begin_turn(&mut fx, req);
            start_effect(&mut fx, &certificate);
            let source = committed(&mut fx, false);
            let snapshot = snapshot(&mut fx, source);
            release_summary(&mut fx, snapshot);
            fx
        } else {
            let mut fx = ready(false);
            let req = break_request(&fx, false);
            begin_turn(&mut fx, req);
            fx
        };
        assert!((0..128).any(|_| {
            advance(&mut fx).unwrap();
            fx.run()
                .context
                .plan_port_work()
                .unwrap()
                .test_stage()
                .starts_with("BreakReport")
        }));
        assert!(fx.records(Kind::PlanBreak).is_empty());
        fx.boot.member.fill_trace();
        let run = fx.run();
        let bound = run.context.plan_port_bound().unwrap();
        // Admit an earlier observation through the actual recorder. The test
        // does not install a pending flag or manufacture an author ticket.
        assert!(matches!(
            run.context.submit_context_author(
                AuthoredEvent {
                    kind: Kind::ModelMeasurement,
                    subsystem: weaver_trace::Subsystem::Harness,
                    turn: Some(TurnRef("break".into())),
                    source: AuthoredSource::Runtime,
                    causal_parent: None,
                    payload: Some(Payload::ModelMeasurement(
                        serde_json::value::to_raw_value(&serde_json::json!({
                            "fixture": "earlier observation before the break report"
                        }))
                        .unwrap(),
                    )),
                },
                RecordPurpose::Observation,
                1,
                &mut run.clock,
            ),
            RecordProgress::Pending
        ));
        let prior = run.context.pending_author().unwrap();
        let prior_ticket = prior.ticket;
        let prior_event = format!("{:?}", prior.event);
        assert!(!run.context.serving_observation_author_available());
        for _ in 0..3 {
            assert_eq!(advance(&mut fx).unwrap(), PlanPortProgress::Pending);
            assert!(take(&mut fx).is_none());
            assert!(fx.records(Kind::PlanBreak).is_empty());
            let context = &fx.run().context;
            assert_eq!(context.plan_port_bound(), Some(bound));
            let prior = context.pending_author().unwrap();
            assert_eq!(prior.ticket, prior_ticket);
            assert_eq!(format!("{:?}", prior.event), prior_event);
            let (_, ticket, sequence, consumed) =
                context.plan_port_work().unwrap().test_refusal_account();
            assert_eq!((ticket, sequence, consumed), (None, None, false));
        }
        fx.drain();
        let run = fx.run();
        let RecordProgress::Recorded {
            sequence: prior_sequence,
            ..
        } = run.context.resume_context_author(&mut run.clock)
        else {
            panic!("the earlier observation must actually reach Recorded")
        };
        assert_eq!(
            run.context.barrier.recorded_author_ticket(prior_sequence),
            Some(prior_ticket)
        );
        assert!(run.context.serving_observation_author_available());
        assert!(fx.records(Kind::PlanBreak).is_empty());
        assert_eq!(advance(&mut fx).unwrap(), PlanPortProgress::Complete);
        let rows = fx.records(Kind::PlanBreak);
        assert_eq!(rows.len(), 1, "Recorded precedes taking BreakRecorded");
        let sequence =
            weaver_trace::Sequence(rows[0]["sequence"].as_str().unwrap().parse().unwrap());
        let run = fx.run();
        let (_, ticket, recorded, consumed) =
            run.context.plan_port_work().unwrap().test_refusal_account();
        assert!(consumed);
        assert_eq!(recorded, Some(sequence));
        assert_eq!(run.context.barrier.recorded_author_ticket(sequence), ticket);
        assert!(ticket.is_some() && ticket != Some(prior_ticket));
        assert!(sequence > prior_sequence);
        assert_eq!(run.context.plan_port_bound(), Some(bound));
        assert!(matches!(
            take(&mut fx),
            Some(PlanPortCompletion::BreakRecorded(_))
        ));
        assert_eq!(fx.files.len(), usize::from(committed_effect));
        fx.boot.member.no_request();
    }
}
