//! Actual released read and recorder proofs, with a scripted member after boot.
use super::*;
use crate::flow::context::plan_port_driver::{
    AllowedAct, CheckedWork, PlanPortAdmission, PlanPortCompletion, PlanPortProgress,
};
use crate::flow::plan::ports::PlanRequest;
use sha2::{Digest as _, Sha256};

const _: fn() = || {
    trait NotClone<M> {
        fn pin() {}
    }
    impl<T: ?Sized> NotClone<()> for T {}
    impl<T: Clone> NotClone<u8> for T {}
    let _ = <AllowedAct as NotClone<_>>::pin;
    let _ = <CheckedWork as NotClone<_>>::pin;
    let _ = <PlanPortCompletion as NotClone<_>>::pin;
};

fn request(seed: &str) -> PlanRequest {
    PlanRequest::WorkReference {
        expected: s::PlanRef {
            plan_id: s::PlanId {
                seed_id: seed.into(),
                local_id: 3.into(),
            },
            revision: 1.into(),
        },
        step_id: s::StepId {
            seed_id: seed.into(),
            local_id: 4.into(),
        },
        span: s::SpanRef {
            seed_id: seed.into(),
            local_id: 5.into(),
            revision: 1.into(),
        },
        through: s::SourcePosition {
            block_id: s::SourceBlockId {
                run: "work-run".into(),
                sequence: 22.into(),
                block: 0.into(),
            },
            turn: None,
        },
    }
}

fn ready(locked: bool) -> SummaryFixture {
    ready_with_seed(locked, false)
}

fn ready_with_seed(locked: bool, foreign: bool) -> SummaryFixture {
    ready_with_status(locked, foreign, s::StepStatus::Active)
}

fn ready_with_status(locked: bool, foreign: bool, status: s::StepStatus) -> SummaryFixture {
    let mut fx = SummaryFixture::new(vec![]);
    let current = locked.then(|| {
        let mut plan = fx.snapshot();
        let PlanRequest::WorkReference {
            expected,
            step_id,
            span,
            through,
        } = request(fx.seed())
        else {
            unreachable!()
        };
        plan.plan = expected;
        plan.status = s::PlanStatus::Locked;
        plan.lock_source = Some(through.block_id.clone());
        plan.categories = vec![s::Category {
            id: "work".into(),
            verbs: vec!["read".into()],
            acts: vec![s::PlanAct::Statement],
            review_required: false,
        }];
        plan.steps = vec![s::PlanStep {
            step_id,
            ordinal: 0.into(),
            instruction: s::StatementRef {
                block_id: through.block_id,
                binding: 0.into(),
            },
            category: "work".into(),
            group: None,
            status,
            span: (status == s::StepStatus::Active).then_some(span),
            outcome_span: None,
            review_call: None,
            work: None,
            closure_source: None,
        }];
        if foreign {
            let seed = "fedcba9876543210fedcba9876543210";
            plan.plan.plan_id.seed_id = seed.into();
            plan.task.task.seed_id = seed.into();
            plan.task.prompt.seed_id = seed.into();
            plan.steps[0].step_id.seed_id = seed.into();
            plan.steps[0].span.as_mut().unwrap().seed_id = seed.into();
        }
        plan
    });
    let _proof = fx.boot(current);
    let run = fx.run();
    assert!(run.context.complete_wait(PumpDeadline::Enter).is_some());
    let bound = AbsoluteBound::after(run.clock.now().unwrap(), 5_000_000_000).unwrap();
    run.context
        .arm(PumpDeadline::Decision, bound, &mut run.clock)
        .unwrap();
    fx
}

fn begin(fx: &mut SummaryFixture, request: PlanRequest) {
    let run = fx.run();
    run.context
        .begin_plan_operation(request, None, &mut run.clock)
        .unwrap();
}
fn advance(fx: &mut SummaryFixture) -> Result<PlanPortProgress, RunFailure> {
    let run = fx.run();
    run.context
        .advance_plan_operation(&mut ports(&mut run.clock))
}
fn take(fx: &mut SummaryFixture) -> Option<PlanPortCompletion> {
    let run = fx.run();
    run.context.take_plan_operation(&mut run.clock).unwrap()
}
fn finish(fx: &mut SummaryFixture) -> PlanPortCompletion {
    for _ in 0..128 {
        advance(fx).unwrap();
        if let Some(done) = take(fx) {
            return done;
        }
    }
    panic!("plan port did not finish its recorded account")
}
fn next(fx: &mut SummaryFixture) -> s::StateFrame {
    for _ in 0..128 {
        advance(fx).unwrap();
        if !fx.run().context.flow.state().outstanding().is_empty() {
            return fx.boot.member.sent();
        }
    }
    panic!("plan port did not issue its exact required read")
}
fn refuse(code: s::ReadRefusalCode) -> s::StateControlAnswer {
    s::StateControlAnswer::ControlRefused {
        code: s::ControlRefusalCode::Read(code),
        detail: "retained member detail".into(),
    }
}

fn populated_preview(request: &PlanRequest) -> s::QueryResult {
    let cases: Vec<serde_json::Value> =
        serde_json::from_str(include_str!("fixtures/recall-sources.json")).unwrap();
    let case = cases
        .iter()
        .find(|case| case["case"] == "span_preview_repeated_occurrences")
        .unwrap();
    let mut result: s::QueryResult = serde_json::from_value(case["result"].clone()).unwrap();
    let PlanRequest::WorkReference { span, through, .. } = request else {
        unreachable!()
    };
    let s::QueryResult::SpanPreview {
        span: actual,
        through: frontier,
        entries,
    } = &mut result
    else {
        unreachable!()
    };
    *actual = span.clone();
    *frontier = through.clone();
    for entry in entries.iter_mut() {
        entry.selected_span.as_mut().unwrap().span_ref = span.clone();
    }
    assert_eq!(entries.len(), 2);
    assert_eq!(entries[0].pairs.len(), 1);
    assert!(entries[0].pairs[0].terminal_source.is_some());
    result
}

#[test]
fn plan_port_foreign_identity_records_original_without_consuming_cache_or_query() {
    let mut fx = ready(true);
    let original = request("foreign-seed");
    let expected = original.expected().clone();
    let step = original.step_id().clone();
    let recalls = fx.records(Kind::Recall).len();
    begin(&mut fx, original);
    assert!(take(&mut fx).is_none());
    let Some(bound) = fx.run().context.deadlines.get(PumpDeadline::Decision) else {
        panic!()
    };
    assert!(
        fx.run()
            .context
            .complete_wait(PumpDeadline::Decision)
            .is_none()
    );
    let PlanPortCompletion::Refused { refusal, .. } = finish(&mut fx) else {
        panic!("foreign request cannot grant work")
    };
    assert_eq!(refusal.case, s::PlanOperationCase::InvalidTarget);
    assert_eq!(refusal.read_code, None);
    let rows = fx.records(Kind::Refusal);
    assert_eq!(rows.len(), 1);
    assert!(rows[0].get("causal_parent").is_none());
    assert!(rows[0].get("turn").is_none());
    assert_eq!(
        rows[0]["payload"],
        serde_json::to_value(weaver_types::RefusalRecord::PlanOperation {
            operation: s::PlanPortOperation::WorkReference,
            expected,
            step_id: Some(step),
            refusal,
        })
        .unwrap()
    );
    assert_eq!(fx.records(Kind::Recall).len(), recalls);
    assert!(fx.run().context.flow.state().outstanding().is_empty());
    assert_eq!(
        fx.run().context.deadlines.get(PumpDeadline::Decision),
        Some(bound)
    );
}

#[test]
fn plan_port_no_locked_plan_consumes_cached_proof_before_its_recorded_refusal() {
    let mut fx = ready(false);
    let parent = fx
        .run()
        .context
        .plan_cache
        .committed
        .as_ref()
        .unwrap()
        .read()
        .recall_sequence();
    let req = request(fx.seed());
    begin(&mut fx, req);
    let PlanPortCompletion::Refused { refusal, .. } = finish(&mut fx) else {
        panic!()
    };
    assert_eq!(refusal.case, s::PlanOperationCase::NoLockedPlan);
    assert_eq!(refusal.read_code, None);
    let rows = fx.records(Kind::Refusal);
    assert_eq!(rows.len(), 1);
    assert_eq!(rows[0]["causal_parent"], serde_json::json!(parent));
    assert_eq!(fx.run().context.committed_plan_summary().unwrap(), None);
}

#[test]
fn plan_port_work_reference_hashes_actual_released_tagged_preview_and_keeps_decision() {
    let mut fx = ready(true);
    let req = request(fx.seed());
    let bytes = serde_json::to_vec(&populated_preview(&req)).unwrap();
    let expected_hash = format!("{:x}", Sha256::digest(&bytes));
    let selector = req.selector();
    let bound = fx.run().context.deadlines.get(PumpDeadline::Decision);
    begin(&mut fx, req);
    let read = next(&mut fx);
    assert!(
        matches!(&read, s::StateFrame::Control { frame: s::StateControl::Read { selector: actual }, .. } if actual == &selector)
    );
    fx.boot
        .member
        .respond(&read, page(61, 0, bytes.len(), &bytes));
    let release = next(&mut fx);
    assert!(
        take(&mut fx).is_none(),
        "unreleased preview grants no WorkRef"
    );
    fx.boot.member.respond(
        &release,
        s::StateControlAnswer::Released {
            snapshot_id: 61.into(),
        },
    );
    let PlanPortCompletion::Work(work) = finish(&mut fx) else {
        panic!("successful preview must construct checked work")
    };
    assert_eq!(work.reference().content_sha256.to_string(), expected_hash);
    assert_eq!(
        fx.run().context.deadlines.get(PumpDeadline::Decision),
        bound
    );
    assert!(fx.records(Kind::Refusal).is_empty());
    assert!(fx.records(Kind::PlanEvent).is_empty());
}

#[test]
fn plan_port_initial_read_refusals_keep_exact_codes_and_recorded_measurement() {
    use s::{PlanOperationCase as Case, ReadRefusalCode as Code};
    for (code, case) in [
        (Code::CounterExhausted, Case::ReadRefused),
        (Code::ResultTooLarge, Case::ReadRefused),
        (Code::ResultUnavailable, Case::ReadRefused),
        (Code::AnswerTooLarge, Case::ReadRefused),
        (Code::InvalidSelector, Case::WorkChanged),
        (Code::SpanPending, Case::WorkChanged),
        (Code::PartialPair, Case::WorkChanged),
    ] {
        let mut fx = ready(true);
        let req = request(fx.seed());
        begin(&mut fx, req);
        let read = next(&mut fx);
        fx.boot.member.respond(&read, refuse(code));
        let PlanPortCompletion::Refused { refusal, .. } = finish(&mut fx) else {
            panic!()
        };
        assert_eq!(
            refusal,
            s::PlanOperationRefused {
                case,
                read_code: Some(code),
                detail: "retained member detail".into()
            }
        );
        let rows = fx.records(Kind::Refusal);
        assert_eq!(rows.len(), 1);
        assert_eq!(
            rows[0]["causal_parent"],
            fx.records(Kind::StateStoreMeasurement).last().unwrap()["sequence"]
        );
        assert!(fx.run().context.flow.state().outstanding().is_empty());
        assert!(fx.records(Kind::PlanEvent).is_empty());
    }
}

#[test]
fn plan_port_next_refusal_releases_acquired_snapshot_before_accounting() {
    for code in [
        s::ReadRefusalCode::UnknownSnapshot,
        s::ReadRefusalCode::WrongOffset,
        s::ReadRefusalCode::ResultUnavailable,
        s::ReadRefusalCode::AnswerTooLarge,
    ] {
        let mut fx = ready(true);
        let req = request(fx.seed());
        begin(&mut fx, req);
        let read = next(&mut fx);
        fx.boot
            .member
            .respond(&read, page(71, 0, 100, b"{\"span_preview\":"));
        let request = next(&mut fx);
        assert!(matches!(
            request,
            s::StateFrame::Control {
                frame: s::StateControl::Next { .. },
                ..
            }
        ));
        fx.boot.member.respond(&request, refuse(code));
        let release = next(&mut fx);
        assert!(
            matches!(release, s::StateFrame::Control { frame: s::StateControl::Release { snapshot_id }, .. } if snapshot_id.0 == 71)
        );
        assert!(take(&mut fx).is_none());
        assert!(fx.records(Kind::Refusal).is_empty());
        let refusing_parent =
            fx.records(Kind::StateStoreMeasurement).last().unwrap()["sequence"].clone();
        fx.boot.member.respond(
            &release,
            s::StateControlAnswer::Released {
                snapshot_id: 71.into(),
            },
        );
        let PlanPortCompletion::Refused { refusal, .. } = finish(&mut fx) else {
            panic!()
        };
        assert_eq!(refusal.case, s::PlanOperationCase::ReadRefused);
        assert_eq!(refusal.detail, "retained member detail");
        assert_eq!(refusal.read_code, Some(code));
        let rows = fx.records(Kind::Refusal);
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0]["causal_parent"], refusing_parent);
        assert_ne!(
            refusing_parent,
            fx.records(Kind::StateStoreMeasurement).last().unwrap()["sequence"],
            "release measurement cannot replace the refusing leg"
        );
    }
}

#[test]
fn plan_port_cancelled_chosen_refusal_still_records_and_grants_no_result() {
    let mut fx = ready(false);
    let req = request(fx.seed());
    begin(&mut fx, req);
    assert_eq!(advance(&mut fx).unwrap(), PlanPortProgress::Progress);
    fx.run().context.cancel_plan_operation();
    assert!(matches!(finish(&mut fx), PlanPortCompletion::Cancelled(_)));
    assert_eq!(fx.records(Kind::Refusal).len(), 1);
    assert!(take(&mut fx).is_none());
}

#[test]
fn plan_port_failed_release_retains_the_owner_instead_of_returning_a_refusal() {
    let mut fx = ready(true);
    let req = request(fx.seed());
    begin(&mut fx, req);
    let read = next(&mut fx);
    fx.boot
        .member
        .respond(&read, page(73, 0, 100, b"{\"span_preview\":"));
    let request = next(&mut fx);
    fx.boot
        .member
        .respond(&request, refuse(s::ReadRefusalCode::ResultUnavailable));
    let release = next(&mut fx);
    assert!(
        matches!(release, s::StateFrame::Control { frame: s::StateControl::Release { snapshot_id }, .. } if snapshot_id.0 == 73)
    );
    assert!(take(&mut fx).is_none());
    fx.boot
        .member
        .respond(&release, refuse(s::ReadRefusalCode::UnknownSnapshot));
    let failure = (0..128)
        .find_map(|_| advance(&mut fx).err())
        .expect("failed release must keep its foundation failure");
    assert!(matches!(
        failure,
        RunFailure::Flow {
            reason: FlowHaltReason::ProtocolFailed,
            ..
        }
    ));
    let run = fx.run();
    assert_eq!(run.context.failure(), Some(&failure));
    assert!(run.context.plan_port_work().is_some());
    assert!(run.context.read.is_some());
    assert!(run.context.take_plan_operation(&mut run.clock).is_err());
    assert!(fx.records(Kind::Refusal).is_empty());
}

#[test]
fn plan_port_open_pair_or_changed_frontier_never_grants_work() {
    for open_pair in [false, true] {
        let mut fx = ready(true);
        let req = request(fx.seed());
        let mut result = populated_preview(&req);
        let s::QueryResult::SpanPreview {
            through, entries, ..
        } = &mut result
        else {
            unreachable!()
        };
        if open_pair {
            let pair = &mut entries[0].pairs[0];
            pair.fate = s::PairFate::Opened;
            pair.reason = None;
            pair.terminal_source = None;
        } else {
            through.block_id.sequence.0 += 1;
        }
        let bytes = serde_json::to_vec(&result).unwrap();
        begin(&mut fx, req);
        let read = next(&mut fx);
        fx.boot
            .member
            .respond(&read, page(75, 0, bytes.len(), &bytes));
        let failure = (0..128)
            .find_map(|_| advance(&mut fx).err())
            .expect("invalid preview cannot grant a work reference");
        assert!(matches!(
            failure,
            RunFailure::Flow {
                reason: FlowHaltReason::ProtocolFailed,
                ..
            }
        ));
        let run = fx.run();
        assert!(run.context.plan_port_work().is_some());
        assert!(run.context.take_plan_operation(&mut run.clock).is_err());
        assert!(fx.records(Kind::Refusal).is_empty());
    }
}

#[test]
fn plan_port_busy_returns_unaccepted_request_without_new_trace_or_wait() {
    let mut fx = ready(true);
    let req = request(fx.seed());
    begin(&mut fx, req);
    let second = request("foreign-second-request");
    let before = format!("{second:?}");
    let run = fx.run();
    let bound = run.context.deadlines.get(PumpDeadline::Decision);
    let error = run
        .context
        .begin_plan_operation(second, None, &mut run.clock)
        .unwrap_err();
    assert_eq!(error.refusal, PlanPortAdmission::Busy);
    assert_eq!(format!("{:?}", error.request), before);
    assert!(error.failure.is_none());
    assert_eq!(run.context.deadlines.get(PumpDeadline::Decision), bound);
    assert!(fx.records(Kind::Refusal).is_empty());
    fx.run().context.cancel_plan_operation();
    assert!(matches!(finish(&mut fx), PlanPortCompletion::Cancelled(_)));
}

#[test]
fn plan_port_revision_and_active_step_mismatches_use_cached_parent_without_reading() {
    for revision in [true, false] {
        let mut fx = ready(true);
        let parent = fx
            .run()
            .context
            .plan_cache
            .committed
            .as_ref()
            .unwrap()
            .read()
            .recall_sequence();
        let mut req = request(fx.seed());
        let PlanRequest::WorkReference {
            expected, step_id, ..
        } = &mut req
        else {
            unreachable!()
        };
        if revision {
            expected.revision = 2.into();
        } else {
            step_id.local_id = 90.into();
        }
        begin(&mut fx, req);
        let PlanPortCompletion::Refused { refusal, .. } = finish(&mut fx) else {
            panic!()
        };
        assert_eq!(refusal.case, s::PlanOperationCase::InvalidTarget);
        assert_eq!(refusal.read_code, None);
        assert_eq!(
            fx.records(Kind::Refusal)[0]["causal_parent"],
            serde_json::json!(parent)
        );
        fx.boot.member.no_request();
    }
}

#[test]
fn plan_port_waiting_step_cannot_grant_an_act_or_start_its_statement_read() {
    let mut fx = ready_with_status(true, false, s::StepStatus::Waiting);
    let (request, _) = statement_request(&fx, s::PlanAct::Statement);
    begin(&mut fx, request);
    let PlanPortCompletion::Refused { refusal, .. } = finish(&mut fx) else {
        panic!("a waiting step grants no act");
    };
    assert_eq!(refusal.case, s::PlanOperationCase::InvalidTarget);
    assert_eq!(refusal.read_code, None);
    fx.boot.member.no_request();
}

#[test]
fn plan_port_cancelled_acquired_read_releases_and_grants_no_work_or_refusal() {
    let mut fx = ready(true);
    let req = request(fx.seed());
    let PlanRequest::WorkReference { span, through, .. } = &req else {
        unreachable!()
    };
    let bytes = serde_json::to_vec(&s::QueryResult::SpanPreview {
        span: span.clone(),
        through: through.clone(),
        entries: vec![],
    })
    .unwrap();
    begin(&mut fx, req);
    let read = next(&mut fx);
    fx.run().context.cancel_plan_operation();
    fx.boot
        .member
        .respond(&read, page(91, 0, bytes.len(), &bytes));
    let release = next(&mut fx);
    assert!(
        matches!(release, s::StateFrame::Control { frame: s::StateControl::Release { snapshot_id }, .. } if snapshot_id.0 == 91)
    );
    assert!(take(&mut fx).is_none());
    fx.boot.member.respond(
        &release,
        s::StateControlAnswer::Released {
            snapshot_id: 91.into(),
        },
    );
    assert!(matches!(finish(&mut fx), PlanPortCompletion::Cancelled(_)));
    assert!(fx.records(Kind::Refusal).is_empty());
    assert!(fx.run().context.read.is_none());
}

#[test]
fn plan_port_impossible_selector_refusal_latches_failure_and_retains_operation() {
    for code in [s::ReadRefusalCode::WrongSeed, s::ReadRefusalCode::NotFound] {
        let mut fx = ready(true);
        let req = request(fx.seed());
        begin(&mut fx, req);
        let read = next(&mut fx);
        fx.boot.member.respond(&read, refuse(code));
        let failure = (0..128)
            .find_map(|_| advance(&mut fx).err())
            .expect("impossible read code must fail");
        assert!(matches!(
            failure,
            RunFailure::Flow {
                reason: FlowHaltReason::ProtocolFailed,
                ..
            }
        ));
        let run = fx.run();
        assert!(run.context.take_plan_operation(&mut run.clock).is_err());
        assert!(run.context.plan_port_work().is_some());
        assert!(fx.records(Kind::Refusal).is_empty());
    }
}

#[test]
fn plan_port_expiry_keeps_original_owner_and_grants_no_result() {
    let mut fx = ready(false);
    let req = request(fx.seed());
    begin(&mut fx, req);
    let run = fx.run();
    let bound = run.context.plan_port_bound().unwrap();
    run.clock.advance_to(bound.deadline_ns()).unwrap();
    let failure = advance(&mut fx).unwrap_err();
    assert!(matches!(
        failure,
        RunFailure::Flow {
            reason: FlowHaltReason::DecisionBound,
            ..
        }
    ));
    let run = fx.run();
    assert!(run.context.plan_port_work().is_some());
    assert!(run.context.take_plan_operation(&mut run.clock).is_err());
    assert_eq!(
        run.context.deadlines.get(PumpDeadline::Decision),
        Some(bound)
    );
}

fn statement_request(fx: &SummaryFixture, act: s::PlanAct) -> (PlanRequest, Vec<u8>) {
    let cases: Vec<serde_json::Value> =
        serde_json::from_str(include_str!("fixtures/recall-sources.json")).unwrap();
    let case = cases
        .iter()
        .find(|case| case["case"] == "statement_original_ordinal")
        .unwrap();
    let s::ReadSelector::Statement { statement } =
        serde_json::from_value(case["selector"].clone()).unwrap()
    else {
        unreachable!()
    };
    let PlanRequest::WorkReference {
        expected, step_id, ..
    } = request(fx.seed())
    else {
        unreachable!()
    };
    (
        PlanRequest::AssessAct {
            expected,
            step_id,
            statement,
            act,
        },
        serde_json::to_vec(&case["result"]).unwrap(),
    )
}

#[test]
fn plan_port_assess_act_uses_exact_held_binding_and_loaded_tool_grant() {
    for (act, expected_case) in [
        (s::PlanAct::Statement, None),
        (
            s::PlanAct::InwardRecall {
                verb: "read".into(),
            },
            Some(s::PlanOperationCase::WrongCategory),
        ),
        (
            s::PlanAct::ExternalTool { name: "sh".into() },
            Some(s::PlanOperationCase::UnknownTool),
        ),
    ] {
        let mut fx = ready(true);
        let (req, bytes) = statement_request(&fx, act.clone());
        let expected_request = format!("{req:?}");
        begin(&mut fx, req);
        let read = next(&mut fx);
        assert!(
            matches!(&read, s::StateFrame::Control { frame: s::StateControl::Read { selector: s::ReadSelector::Statement { statement } }, .. } if statement.binding.0 == 7)
        );
        fx.boot
            .member
            .respond(&read, page(81, 0, bytes.len(), &bytes));
        let release = next(&mut fx);
        assert!(take(&mut fx).is_none());
        fx.boot.member.respond(
            &release,
            s::StateControlAnswer::Released {
                snapshot_id: 81.into(),
            },
        );
        match (finish(&mut fx), expected_case) {
            (PlanPortCompletion::Allowed(grant), None) => {
                assert_eq!(format!("{:?}", grant.request()), expected_request);
                assert!(fx.records(Kind::Refusal).is_empty());
            }
            (PlanPortCompletion::Refused { refusal, .. }, Some(case)) => {
                assert_eq!(refusal.case, case);
                assert_eq!(refusal.read_code, None);
                let rows = fx.records(Kind::Refusal);
                assert_eq!(rows.len(), 1);
                assert_eq!(
                    rows[0]["causal_parent"],
                    fx.records(Kind::Recall).last().unwrap()["sequence"]
                );
                assert_eq!(rows[0]["payload"]["operation"], "assess_act");
            }
            _ => panic!("act predicate granted a different outcome"),
        }
        assert!(fx.records(Kind::PlanEvent).is_empty());
    }
}

#[test]
fn plan_port_statement_not_found_retains_read_code_and_measurement_parent() {
    let mut fx = ready(true);
    let (req, _) = statement_request(&fx, s::PlanAct::Statement);
    begin(&mut fx, req);
    let read = next(&mut fx);
    fx.boot
        .member
        .respond(&read, refuse(s::ReadRefusalCode::NotFound));
    let PlanPortCompletion::Refused { refusal, .. } = finish(&mut fx) else {
        panic!()
    };
    assert_eq!(refusal.case, s::PlanOperationCase::InvalidTarget);
    assert_eq!(refusal.read_code, Some(s::ReadRefusalCode::NotFound));
    assert_eq!(
        fx.records(Kind::Refusal)[0]["causal_parent"],
        fx.records(Kind::StateStoreMeasurement).last().unwrap()["sequence"]
    );
    fx.boot.member.no_request();
}

#[test]
fn plan_port_foreign_cached_seed_fails_instead_of_refusing_a_local_target() {
    let mut fx = ready_with_seed(true, true);
    let req = request(fx.seed());
    begin(&mut fx, req);
    assert!(matches!(
        advance(&mut fx),
        Err(RunFailure::Flow {
            reason: FlowHaltReason::ProtocolFailed,
            ..
        })
    ));
    let run = fx.run();
    assert!(run.context.plan_port_work().is_some());
    assert!(run.context.take_plan_operation(&mut run.clock).is_err());
    assert!(fx.records(Kind::Refusal).is_empty());
    fx.boot.member.no_request();
}
