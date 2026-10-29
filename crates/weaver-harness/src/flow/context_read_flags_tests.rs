//! Explicit flag sampling uses the real paged read and recorder, not SQL copies.
use super::*;
use crate::flow::context::read_driver::flags::{FlagObservation, RecordedFlags};

fn plan(member: &Member) -> s::PlanRef {
    s::PlanRef {
        plan_id: s::PlanId {
            seed_id: member
                .run
                .context
                .open_start
                .as_ref()
                .unwrap()
                .seed_id
                .clone(),
            local_id: 3.into(),
        },
        revision: 2.into(),
    }
}
fn after() -> s::SourceBlockId {
    s::SourceBlockId {
        run: "z-prior-run".into(),
        sequence: 9.into(),
        block: 0.into(),
    }
}
fn snapshot(member: &Member) -> s::PlanFlagSnapshot {
    s::PlanFlagSnapshot {
        plan: plan(member),
        after: after(),
        through: s::SourcePosition {
            block_id: s::SourceBlockId {
                run: "a-later-run".into(),
                sequence: 4.into(),
                block: 0.into(),
            },
            turn: None,
        },
        total_flags: 7.into(),
        matched_flags: 4.into(),
        unmatched_flags: 3.into(),
    }
}
fn begin(member: &mut Member) -> s::StateFrame {
    member
        .run
        .context
        .observe_flags(plan(member), after(), &mut member.run.clock)
        .unwrap();
    member.next_request()
}
fn finish(member: &mut Member) -> RecordedFlags {
    for _ in 0..128 {
        member.advance().unwrap();
        if let Some(done) = member
            .run
            .context
            .take_observed_flags(&mut member.run.clock)
            .unwrap()
        {
            return done;
        }
    }
    panic!("explicit flag observation did not finish");
}

#[test]
fn flags_observation_pages_release_recall_and_measurement_keep_initial_request() {
    let mut member = Member::new();
    let expected = snapshot(&member);
    let bytes = serde_json::to_vec(&s::QueryResult::PlanFlags {
        snapshot: expected.clone(),
    })
    .unwrap();
    let initial = begin(&mut member);
    let original_bound = member.bound();
    let request_id = initial.common().request_id;
    member.respond(&initial, page(13, 0, bytes.len(), &bytes[..31]));
    let next = member.next_request();
    member.respond(&next, page(13, 31, bytes.len(), &bytes[31..]));
    let release = member.next_request();
    assert!(member.records(Kind::StatePlanMeasurement).is_empty());
    assert_eq!(member.bound(), original_bound);
    assert!(matches!(
        member.run.context.take_read(&mut member.run.clock),
        Err(RunFailure::ReadOwned)
    ));
    member.respond(
        &release,
        s::StateControlAnswer::Released {
            snapshot_id: 13.into(),
        },
    );
    let done = finish(&mut member);
    assert_eq!(done.request_id(), request_id);
    assert_eq!(
        done.observation(),
        Some(&FlagObservation::Measured(expected.clone()))
    );
    assert!(done.belongs_to(&member.run.context));
    assert!(!done.belongs_to(&Member::new().run.context));
    let records = member.records(Kind::StatePlanMeasurement);
    assert_eq!(records.len(), 1);
    let row = &records[0];
    assert_eq!(row["sequence"], done.sequence().0.to_string());
    assert_eq!(row["origin"], "loop");
    assert_eq!(row["channel"], "internal");
    assert_eq!(
        row["payload"],
        serde_json::json!({
            "instrument_version":"plan-observation/1", "plan":expected.plan,
            "event":{"flags":{"after":after(),"request_id":request_id,"observation":{"measured":{"snapshot":expected}}}}
        })
    );
    assert_eq!(member.records(Kind::Recall).len(), 1);
    assert_eq!(member.requests.len(), 3);
    member.no_request();
}

#[test]
fn flags_observation_initial_refusal_is_recorded_unavailable_without_zero_or_recall() {
    for code in [
        s::ReadRefusalCode::WrongSeed,
        s::ReadRefusalCode::NotFound,
        s::ReadRefusalCode::InvalidSelector,
        s::ReadRefusalCode::ResultUnavailable,
        s::ReadRefusalCode::AnswerTooLarge,
    ] {
        let mut member = Member::new();
        let initial = begin(&mut member);
        member.respond(&initial, refused(code));
        let done = finish(&mut member);
        assert_eq!(
            done.observation(),
            Some(&FlagObservation::Unavailable(code))
        );
        assert_eq!(done.request_id(), initial.common().request_id);
        assert!(member.run.context.failure().is_none());
        assert!(member.records(Kind::Recall).is_empty());
        let rows = member.records(Kind::StatePlanMeasurement);
        assert_eq!(rows.len(), 1);
        assert_eq!(
            rows[0]["payload"]["event"]["flags"]["observation"],
            serde_json::json!({"unavailable":{"code":code}})
        );
        assert_eq!(member.requests.len(), 1);
        member.no_request();
    }
}

fn release_to_recall(member: &mut Member) -> s::StateFrame {
    let bytes = serde_json::to_vec(&s::QueryResult::PlanFlags {
        snapshot: snapshot(member),
    })
    .unwrap();
    let initial = begin(member);
    member.respond(&initial, page(13, 0, bytes.len(), &bytes));
    let release = member.next_request();
    member.respond(
        &release,
        s::StateControlAnswer::Released {
            snapshot_id: 13.into(),
        },
    );
    member.record_completion_only();
    member
        .run
        .context
        .settle_recorded_read(&mut member.run.clock)
        .unwrap();
    assert_eq!(member.advance().unwrap(), ReadProgress::Progress);
    assert_eq!(member.records(Kind::Recall).len(), 1);
    assert!(member.records(Kind::StatePlanMeasurement).is_empty());
    initial
}

#[test]
fn flags_observation_next_refusal_retains_first_request_and_waits_for_release() {
    for code in [
        s::ReadRefusalCode::UnknownSnapshot,
        s::ReadRefusalCode::WrongOffset,
        s::ReadRefusalCode::ResultUnavailable,
        s::ReadRefusalCode::AnswerTooLarge,
    ] {
        for cancelled in [false, true] {
            let mut member = Member::new();
            let initial = begin(&mut member);
            let bound = member.bound();
            member.respond(&initial, page(13, 0, 100, b"{"));
            let next = member.next_request();
            member.respond(&next, refused(code));
            let release = member.next_request();
            let refusing_sequence =
                member.records(Kind::StateStoreMeasurement).last().unwrap()["sequence"].clone();
            assert!(member.records(Kind::StatePlanMeasurement).is_empty());
            assert!(
                member
                    .run
                    .context
                    .take_observed_flags(&mut member.run.clock)
                    .unwrap()
                    .is_none()
            );
            assert_eq!(member.bound(), bound);
            if cancelled {
                member
                    .run
                    .context
                    .cancel_read(&mut member.run.clock)
                    .unwrap();
            }
            member.respond(
                &release,
                s::StateControlAnswer::Released {
                    snapshot_id: 13.into(),
                },
            );
            let done = finish(&mut member);
            assert_eq!(done.request_id(), initial.common().request_id);
            assert_eq!(
                done.observation(),
                (!cancelled).then_some(&FlagObservation::Unavailable(code))
            );
            assert!(member.records(Kind::Recall).is_empty());
            let rows = member.records(Kind::StatePlanMeasurement);
            assert_eq!(rows.len(), 1);
            assert_eq!(
                rows[0]["payload"]["event"]["flags"]["request_id"],
                serde_json::json!(initial.common().request_id)
            );
            assert_eq!(rows[0]["causal_parent"], refusing_sequence);
            assert!(member.run.context.failure().is_none());
            assert_eq!(member.requests.len(), 3);
            member.no_request();
        }
    }
}

#[test]
fn flags_observation_pressure_keeps_exact_ticket_sample_bound_and_cancelled_account() {
    for cancelled in [false, true] {
        let mut member = Member::new();
        let initial = release_to_recall(&mut member);
        let bound = member.bound();
        member.fill_trace();
        assert_eq!(member.advance().unwrap(), ReadProgress::Pending);
        let ticket = member.run.context.pending_author().unwrap().ticket;
        let original = member.run.context.barrier.pending().unwrap().clone();
        if cancelled {
            member
                .run
                .context
                .cancel_read(&mut member.run.clock)
                .unwrap();
        }
        for _ in 0..3 {
            assert_eq!(member.advance().unwrap(), ReadProgress::Pending);
            assert_eq!(member.run.context.pending_author().unwrap().ticket, ticket);
            assert_eq!(
                serde_json::to_value(member.run.context.barrier.pending().unwrap()).unwrap(),
                serde_json::to_value(&original).unwrap()
            );
            assert_eq!(member.bound(), bound);
            assert!(
                member
                    .run
                    .context
                    .take_observed_flags(&mut member.run.clock)
                    .unwrap()
                    .is_none()
            );
            assert_eq!(
                member.run.context.complete_wait(PumpDeadline::StateControl),
                None
            );
            assert_eq!(member.requests.len(), 2);
        }
        assert!(member.records(Kind::StatePlanMeasurement).is_empty());
        super::recall_tests::drain_admitted(&mut member);
        let done = finish(&mut member);
        assert_eq!(done.request_id(), initial.common().request_id);
        assert_eq!(done.observation().is_none(), cancelled);
        assert_eq!(member.records(Kind::StatePlanMeasurement).len(), 1);
        assert!(
            member
                .run
                .context
                .take_observed_flags(&mut member.run.clock)
                .unwrap()
                .is_none()
        );
        assert!(
            member
                .run
                .context
                .deadlines
                .get(PumpDeadline::StateControl)
                .is_none()
        );
        member.no_request();
    }
}

#[test]
fn flags_observation_admission_returns_inputs_and_cannot_steal_or_sample_an_ordinary_read() {
    use crate::flow::context::read_driver::flags::FlagsBeginRefusal;
    let mut member = Member::new();
    let mut wrong = plan(&member);
    wrong.plan_id.seed_id = "foreign-seed".into();
    let refused = member
        .run
        .context
        .observe_flags(wrong.clone(), after(), &mut member.run.clock)
        .unwrap_err();
    assert_eq!(
        refused.request,
        s::ReadSelector::PlanFlags {
            plan: wrong,
            after: after()
        }
    );
    assert_eq!(refused.refusal, FlagsBeginRefusal::InvalidAttribution);
    assert!(refused.failure.is_none());
    assert!(member.run.context.read.is_none());
    member.no_request();
    let selector = s::ReadSelector::PlanFlags {
        plan: plan(&member),
        after: after(),
    };
    member
        .run
        .context
        .begin_read(selector.clone(), None, &mut member.run.clock)
        .unwrap();
    let initial = member.next_request();
    let bound = member.bound();
    let refused = member
        .run
        .context
        .observe_flags(plan(&member), after(), &mut member.run.clock)
        .unwrap_err();
    assert_eq!(refused.refusal, FlagsBeginRefusal::Busy);
    assert_eq!(refused.request, selector);
    assert_eq!(member.bound(), bound);
    assert!(matches!(
        member
            .run
            .context
            .take_observed_flags(&mut member.run.clock),
        Err(RunFailure::ReadOwned)
    ));
    member.respond(&initial, super::refused(s::ReadRefusalCode::NotFound));
    assert!(member.finish().is_err());
    assert!(member.records(Kind::StatePlanMeasurement).is_empty());
    assert!(member.run.context.failure().is_none());
    assert_eq!(member.requests.len(), 1);
}

#[test]
fn flags_observation_wrong_echo_or_count_partition_is_a_component_failure() {
    for defect in 0..4 {
        let mut member = Member::new();
        let mut result = snapshot(&member);
        match defect {
            0 => result.plan.revision = 3.into(),
            1 => result.after.sequence = 10.into(),
            2 => result.total_flags = 8.into(),
            3 => {
                result.matched_flags = u64::MAX.into();
                result.unmatched_flags = 1.into();
                result.total_flags = 0.into();
            }
            _ => unreachable!(),
        }
        let bytes = serde_json::to_vec(&s::QueryResult::PlanFlags { snapshot: result }).unwrap();
        let initial = begin(&mut member);
        member.respond(&initial, page(13, 0, bytes.len(), &bytes));
        member.assert_component_failure();
        assert!(member.records(Kind::StatePlanMeasurement).is_empty());
        assert!(member.records(Kind::Recall).is_empty());
        assert!(
            member
                .run
                .context
                .take_observed_flags(&mut member.run.clock)
                .is_err()
        );
    }
}

#[test]
fn flags_observation_pressure_cannot_renew_the_original_read_bound() {
    let mut member = Member::new();
    release_to_recall(&mut member);
    let bound = member.bound();
    member.fill_trace();
    assert_eq!(member.advance().unwrap(), ReadProgress::Pending);
    member.run.clock.advance_to(bound.deadline_ns()).unwrap();
    assert!(matches!(
        member.advance(),
        Err(RunFailure::Flow {
            reason: FlowHaltReason::ControlBound,
            ..
        })
    ));
    assert!(member.records(Kind::StatePlanMeasurement).is_empty());
    assert!(
        member
            .run
            .context
            .take_observed_flags(&mut member.run.clock)
            .is_err()
    );
    assert!(member.run.context.read.is_some());
    assert_eq!(member.requests.len(), 2);
}
