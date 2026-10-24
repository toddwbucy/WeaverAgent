use super::*;
use std::time::{Duration, Instant};

#[path = "context_read_channel_tests.rs"]
mod channel_tests;

// A read proof is consumed by its next owner, never duplicated for two consumers.
const _: fn() = || {
    trait ReadProofMustNotClone<Marker> {
        fn check() {}
    }
    impl<T: ?Sized> ReadProofMustNotClone<()> for T {}
    impl<T: Clone> ReadProofMustNotClone<u8> for T {}
    let _ = <RecordedRead as ReadProofMustNotClone<_>>::check;
    let _ = <CancelledRead as ReadProofMustNotClone<_>>::check;
};

fn released(member: &mut Member) {
    let read = member.begin();
    member.respond(&read, page(7, 0, EMPTY.len(), EMPTY));
    let release = member.next_request();
    member.respond(
        &release,
        s::StateControlAnswer::Released {
            snapshot_id: 7.into(),
        },
    );
    member.record_completion_only();
    member
        .run
        .context
        .settle_recorded_read(&mut member.run.clock)
        .unwrap();
    assert!(member.run.context.read_blocks_reduction());
    assert!(member.run.context.flow.operation_completion().is_none());
    assert!(member.records(Kind::Recall).is_empty());
    member.no_result();
}
fn take(member: &mut Member) -> ReadCompletion {
    assert_eq!(member.advance().unwrap(), ReadProgress::Complete);
    member
        .run
        .context
        .take_read(&mut member.run.clock)
        .unwrap()
        .unwrap()
}
fn recall_payload(member: &Member) -> serde_json::Value {
    let records = member.records(Kind::Recall);
    assert_eq!(records.len(), 1);
    let record = &records[0];
    assert_eq!(record["channel"], "internal");
    assert_eq!(record["origin"], "runtime");
    assert_eq!(record["subsystem"], "harness");
    assert!(record.get("turn").is_none_or(serde_json::Value::is_null));
    assert!(
        record
            .get("causal_parent")
            .is_none_or(serde_json::Value::is_null)
    );
    assert!(record["payload"].get("boot").is_none());
    record["payload"]["read"].clone()
}
pub(super) fn drain_admitted(member: &mut Member) {
    // Drain exact bytes already admitted to the real blocked writer. Returning
    // only after they arrive gives the retry a deterministic capacity opening.
    let expected: Vec<u8> = member
        .run
        .context
        .barrier
        .recorder()
        .structure()
        .iter()
        .flat_map(|record| record.line.as_bytes().iter().copied())
        .collect();
    let mut reader = member.run._reader.try_clone().unwrap();
    reader
        .set_read_timeout(Some(Duration::from_secs(10)))
        .unwrap();
    let drain = std::thread::spawn(move || {
        let mut received = vec![0; expected.len()];
        reader.read_exact(&mut received).unwrap();
        assert_eq!(received, expected);
    });
    drain.join().unwrap();
    let watch = Instant::now() + Duration::from_secs(5);
    while member.run.context.barrier.recorder().pressure().queued != 0 {
        assert!(
            Instant::now() < watch,
            "real trace writer did not finish draining"
        );
        std::thread::yield_now();
    }
}

#[test]
fn released_read_retains_exact_provenance_and_original_bound_until_recorded() {
    let mut member = Member::new();
    released(&mut member);
    let bound = member.bound();
    let measurements = member.records(Kind::StateStoreMeasurement).len();
    // This owner already holds the released result, before a barrier ticket
    // exists. Generic reduction must not claim work in that admission gap.
    assert!(member.run.context.pending_author().is_none());
    assert_eq!(
        member
            .run
            .context
            .reduce_flow(&mut member.run.clock)
            .unwrap(),
        Advance::Pending
    );
    assert!(member.records(Kind::Recall).is_empty());
    assert_eq!(
        member.run.context.complete_wait(PumpDeadline::StateControl),
        None
    );
    let ReadCompletion::Recorded(read) = take(&mut member) else {
        panic!("read was not recorded");
    };
    let records = member.records(Kind::Recall);
    assert_eq!(records[0]["sequence"], read.recall_sequence().0.to_string());
    assert_eq!(read.raw_bytes(), EMPTY);
    assert_eq!(read.snapshot_id(), 7.into());
    assert_eq!(
        recall_payload(&member),
        serde_json::json!({
            "selector":{"history":{"noun":"record","verb":null,"condition":null}},
            "snapshot_id":"7", "sources":[], "spans":[],
        })
    );
    assert_eq!(
        member.records(Kind::StateStoreMeasurement).len(),
        measurements
    );
    assert_eq!(member.requests.len(), 2);
    assert!(bound.deadline_ns() > member.now());
    assert!(
        member
            .run
            .context
            .deadlines
            .get(PumpDeadline::StateControl)
            .is_none()
    );
    assert!(member.run.context.flow.ledger().facts().next().is_none());
    member.no_result();
    member.no_request();
}

#[test]
fn plan_summary_carrier_survives_release_and_recall_backpressure() {
    let cases: Vec<serde_json::Value> =
        serde_json::from_str(include_str!("fixtures/recall-sources.json")).unwrap();
    for name in ["plan_complete_traversal", "plan_current_populated"] {
        let case = cases.iter().find(|case| case["case"] == name).unwrap();
        let selector: s::ReadSelector = serde_json::from_value(case["selector"].clone()).unwrap();
        let raw = serde_json::to_vec(&case["result"]).unwrap();
        let mut member = Member::new();
        member
            .run
            .context
            .begin_read(selector, None, &mut member.run.clock)
            .unwrap();
        let request = member.next_request();
        member.respond(&request, page(7, 0, raw.len(), &raw));
        let release = member.next_request();
        member.respond(
            &release,
            s::StateControlAnswer::Released {
                snapshot_id: 7.into(),
            },
        );
        member.record_completion_only();
        member
            .run
            .context
            .settle_recorded_read(&mut member.run.clock)
            .unwrap();
        member.no_result();
        assert!(member.records(Kind::Recall).is_empty());
        let bound = member.bound();
        member.fill_trace();
        assert_eq!(member.advance().unwrap(), ReadProgress::Pending);
        let ticket = member.run.context.pending_author().unwrap().ticket;
        for _ in 0..2 {
            assert_eq!(member.advance().unwrap(), ReadProgress::Pending);
            assert_eq!(member.run.context.pending_author().unwrap().ticket, ticket);
            assert_eq!(member.bound(), bound);
            member.no_result();
        }
        drain_admitted(&mut member);
        let ReadCompletion::Recorded(read) = take(&mut member) else {
            panic!("released plan read must become Recorded");
        };
        assert_eq!(read.raw_bytes(), raw);
        assert_eq!(serde_json::to_value(read.result()).unwrap(), case["result"]);
        assert_eq!(member.records(Kind::Recall).len(), 1);
        assert_eq!(
            member.records(Kind::Recall)[0]["sequence"],
            read.recall_sequence().0.to_string()
        );
        assert_eq!(
            recall_payload(&member)["sources"],
            case["recall"]["read"]["sources"]
        );
        assert_eq!(
            member.requests.len(),
            2,
            "the carrier requires no extra lookup"
        );
        member.no_result();
        member.no_request();
    }
}

#[test]
fn malformed_plan_summary_carrier_retains_the_answer_without_release_or_recall() {
    let cases: Vec<serde_json::Value> =
        serde_json::from_str(include_str!("fixtures/recall-sources.json")).unwrap();
    for name in ["plan_complete_traversal", "plan_current_populated"] {
        let case = cases.iter().find(|case| case["case"] == name).unwrap();
        for defect in 0..8 {
            let mut result = case["result"].clone();
            let snapshot = if name == "plan_current_populated" {
                &mut result["plan_current"]["current"]
            } else {
                &mut result["plan"]["snapshot"]
            };
            match defect {
                0 => {
                    snapshot.as_object_mut().unwrap().remove("document_sha256");
                }
                1 => snapshot["document_sha256"] = serde_json::json!("C".repeat(64)),
                2 => snapshot["last_source"] = snapshot["last_source"]["block_id"].clone(),
                3 => {
                    snapshot["last_source"]
                        .as_object_mut()
                        .unwrap()
                        .remove("turn");
                }
                4 => snapshot["last_source"]["extra"] = serde_json::Value::Null,
                5 => {
                    snapshot["last_source"] = serde_json::json!([
                        snapshot["last_source"]["block_id"],
                        snapshot["last_source"]["turn"]
                    ])
                }
                6 => snapshot["last_source"]["block_id"] = serde_json::json!(["run", "12", "2"]),
                7 => {
                    *snapshot = serde_json::Value::Array(
                        [
                            "plan",
                            "task",
                            "status",
                            "review",
                            "operator_source",
                            "lock_source",
                            "document_sha256",
                            "last_source",
                            "categories",
                            "group_drafts",
                            "groups",
                            "steps",
                            "broken",
                        ]
                        .map(|name| snapshot[name].clone())
                        .into(),
                    )
                }
                _ => unreachable!(),
            }
            let mut member = Member::new();
            let selector = serde_json::from_value(case["selector"].clone()).unwrap();
            member
                .run
                .context
                .begin_read(selector, None, &mut member.run.clock)
                .unwrap();
            let request = member.next_request();
            let raw = serde_json::to_vec(&result).unwrap();
            let answer = member.answer(&request, page(7, 0, raw.len(), &raw));
            // The transport envelope is valid. The completed page is decoded
            // only after the actual operation measurement has been Recorded.
            member.receive(&answer).expect("valid transport envelope");
            let failure = (0..128)
                .find_map(|_| member.advance().err())
                .expect("malformed complete result must fail after its measurement");
            assert!(
                matches!(&failure, RunFailure::Flow {
                reason: FlowHaltReason::ProtocolFailed,
                point: HaltPoint::StateReply,
                detail,
            } if detail == "malformed complete read result"),
                "{name}/{defect}: {failure:?}"
            );
            member.assert_protocol_failure(failure);
            let OperationCompletion::State { completion, .. } = member
                .run
                .context
                .read
                .as_ref()
                .unwrap()
                .rejected_for_test()
                .unwrap()
            else {
                panic!("retained malformed carrier answer");
            };
            assert_eq!(
                serde_json::to_value(&completion.answer).unwrap(),
                serde_json::to_value(&answer).unwrap()
            );
            assert!(member.records(Kind::Recall).is_empty());
            assert_eq!(
                member.requests.len(),
                1,
                "malformed carrier must not issue Release"
            );
        }
    }
}

#[test]
fn recall_pressure_retains_one_ticket_and_cancelled_read_drains_without_consumption() {
    let mut member = Member::new();
    released(&mut member);
    let bound = member.bound();
    member.fill_trace();
    assert_eq!(member.advance().unwrap(), ReadProgress::Pending);
    let ticket = member.run.context.pending_author().unwrap().ticket;
    let exact =
        serde_json::to_string(&member.run.context.pending_author().unwrap().event.payload).unwrap();
    for _ in 0..3 {
        assert_eq!(member.advance().unwrap(), ReadProgress::Pending);
        member.no_result();
        assert_eq!(member.bound(), bound);
        assert_eq!(member.run.context.pending_author().unwrap().ticket, ticket);
        assert_eq!(
            serde_json::to_string(&member.run.context.pending_author().unwrap().event.payload)
                .unwrap(),
            exact
        );
    }
    // Generic authors and coordinator observations cannot take this ticket.
    let foreign = super::super::tests::event(Kind::SessionClosed, None);
    assert_eq!(
        member
            .run
            .context
            .author_and_wait(&mut ports(&mut member.run.clock), foreign)
            .map_err(|failure| failure.retained_reason())
            .unwrap_err(),
        RunFailure::ReadOwned
    );
    assert_eq!(
        member
            .run
            .context
            .resume_author_and_wait(&mut ports(&mut member.run.clock))
            .map_err(|failure| failure.retained_reason())
            .unwrap_err(),
        RunFailure::ReadOwned
    );
    assert_eq!(
        member
            .run
            .context
            .reduce_flow(&mut member.run.clock)
            .unwrap(),
        Advance::Pending
    );
    assert_eq!(member.run.context.pending_author().unwrap().ticket, ticket);
    member
        .run
        .context
        .cancel_read(&mut member.run.clock)
        .unwrap();
    member
        .run
        .context
        .cancel_read(&mut member.run.clock)
        .unwrap();
    assert_eq!(member.advance().unwrap(), ReadProgress::Pending);
    assert!(member.records(Kind::Recall).is_empty());
    assert_eq!(member.requests.len(), 2);
    member.no_request();
    drain_admitted(&mut member);
    let ReadCompletion::Cancelled(cancelled) = take(&mut member) else {
        panic!("revoked read escaped as consumable content");
    };
    let sequence = cancelled
        .recall_sequence()
        .expect("successful cancelled read still records");
    assert!(cancelled.refusal().is_none());
    assert_eq!(
        member.records(Kind::Recall)[0]["sequence"],
        sequence.0.to_string()
    );
    assert_eq!(recall_payload(&member)["sources"], serde_json::json!([]));
    assert!(member.run.context.pending_author().is_none());
    assert_eq!(member.requests.len(), 2);
    member.no_result();
    member.no_request();
    let line = member
        .run
        .context
        .barrier
        .recorder()
        .structure()
        .by_kind(Kind::Recall)
        .next()
        .unwrap()
        .line
        .as_bytes()
        .to_vec();
    let mut received = vec![0; line.len()];
    member.run._reader.read_exact(&mut received).unwrap();
    assert_eq!(received, line);
}

#[test]
fn read_cancellation_keeps_paging_release_and_refusal_dispositions_distinct() {
    for refuse in [false, true] {
        let mut member = Member::new();
        let initial = member.begin();
        member
            .run
            .context
            .cancel_read(&mut member.run.clock)
            .unwrap();
        member.respond(&initial, page(7, 0, EMPTY.len(), &EMPTY[..8]));
        let next = member.next_request();
        if refuse {
            member.respond(&next, refused(s::ReadRefusalCode::ResultUnavailable));
        } else {
            member.respond(&next, page(7, 8, EMPTY.len(), &EMPTY[8..]));
        }
        let release = member.next_request();
        member.respond(
            &release,
            s::StateControlAnswer::Released {
                snapshot_id: 7.into(),
            },
        );
        let completion = loop {
            member.advance().unwrap();
            if let Some(completion) = member.run.context.take_read(&mut member.run.clock).unwrap() {
                break completion;
            }
        };
        let ReadCompletion::Cancelled(cancelled) = completion else {
            panic!("cancelled paging read granted consumption");
        };
        if refuse {
            assert!(
                matches!(cancelled.refusal(), Some(crate::flow::read::ReadFailure::Refused {
                code: s::ReadRefusalCode::ResultUnavailable, detail,
            }) if detail == "scripted read refusal")
            );
            assert!(cancelled.recall_sequence().is_none());
            assert!(member.records(Kind::Recall).is_empty());
        } else {
            assert!(cancelled.refusal().is_none());
            assert!(cancelled.recall_sequence().is_some());
            assert_eq!(member.records(Kind::Recall).len(), 1);
        }
        assert_eq!(member.requests.len(), 3);
        assert!(
            member
                .run
                .context
                .deadlines
                .get(PumpDeadline::StateControl)
                .is_none()
        );
        member.no_result();
        member.no_request();
    }
}

struct RecallClock {
    before: u64,
    expired: u64,
    remaining: usize,
}
impl PumpIo for RecallClock {
    fn now(&mut self) -> std::io::Result<u64> {
        if self.remaining == 0 {
            Ok(self.expired)
        } else {
            self.remaining -= 1;
            Ok(self.before)
        }
    }
    fn wait(&mut self, _: &[WaitInterest<'_>], _: Option<u64>) -> std::io::Result<WaitOutcome> {
        panic!("recall owner performs only bounded local progress");
    }
}

#[test]
fn recall_retains_before_author_and_after_recorded_across_recoverable_leave() {
    // Advance checks live, its recall driver checks live, and preparation reads
    // the clock. The fourth read binds the actual Recorded boundary; the fifth
    // is the trailing lifecycle check after its continuation has been saved.
    for remaining in [1, 4] {
        let mut member = Member::new();
        released(&mut member);
        let bound = member.bound();
        let now = member.now();
        let leave = AbsoluteBound::after(now, 10).unwrap();
        member
            .run
            .context
            .arm(PumpDeadline::Leave, leave, &mut member.run.clock)
            .unwrap();
        let mut clock = RecallClock {
            before: now,
            expired: leave.deadline_ns(),
            remaining,
        };
        assert_eq!(
            member
                .run
                .context
                .advance_read(&mut ports(&mut clock))
                .unwrap_err(),
            RunFailure::Wait {
                key: PumpDeadline::Leave,
                bound: leave
            }
        );
        assert!(member.run.context.halt().is_none());
        assert_eq!(
            member.records(Kind::Recall).len(),
            usize::from(remaining == 4)
        );
        if remaining == 1 {
            assert_eq!(member.bound(), bound);
        }
        assert_eq!(
            member
                .run
                .context
                .advance_read(&mut ports(&mut clock))
                .unwrap_err(),
            RunFailure::Wait {
                key: PumpDeadline::Leave,
                bound: leave
            }
        );
        member
            .run
            .context
            .complete_wait(PumpDeadline::Leave)
            .unwrap();
        member.run.clock.advance_to(leave.deadline_ns()).unwrap();
        let ReadCompletion::Recorded(read) = take(&mut member) else {
            panic!("read lost proof");
        };
        assert_eq!(member.records(Kind::Recall).len(), 1);
        assert_eq!(
            member.records(Kind::Recall)[0]["sequence"],
            read.recall_sequence().0.to_string()
        );
        assert_eq!(member.requests.len(), 2);
        member.no_result();
        member.no_request();
    }
}

#[test]
fn malformed_read_identities_retain_the_received_answer_before_release() {
    let cases: Vec<serde_json::Value> =
        serde_json::from_str(include_str!("fixtures/recall-sources.json")).unwrap();
    for defect in 0..4 {
        let name = match defect {
            0 => "history_filtered_pair_union",
            1 => "marks_sources_only",
            _ => "plan_current_populated",
        };
        let case = cases.iter().find(|case| case["case"] == name).unwrap();
        let selector: s::ReadSelector = serde_json::from_value(case["selector"].clone()).unwrap();
        let mut result: s::QueryResult = serde_json::from_value(case["result"].clone()).unwrap();
        crate::flow::read_validate::validate(&selector, &result).unwrap();
        match &mut result {
            s::QueryResult::Entries { entries } => {
                entries.truncate(1);
                entries[0].pairs.clear();
                entries[0].selected_span = None;
                // T11: only the entry source is malformed. There is no
                // selector/span seed to reject this fixture incidentally.
                entries[0].source = s::ReadSource::Seed(s::SeedSource {
                    seed_id: "not-a-seed".into(),
                    ordinal: 0.into(),
                });
            }
            s::QueryResult::Marks {
                current_task: Some(task),
                ..
            } => {
                // Marks has no selector span echo or forbidden entry metadata
                // that could reject this answer without the revision check.
                task.task.revision = 2.into();
            }
            s::QueryResult::PlanCurrent {
                current: Some(snapshot),
                ..
            } => {
                if defect == 2 {
                    snapshot.plan.plan_id.local_id = 0.into();
                } else {
                    snapshot.steps[0].step_id.local_id = 0.into();
                }
            }
            _ => panic!("fixture shape"),
        }
        let mut member = Member::new();
        member
            .run
            .context
            .begin_read(selector, None, &mut member.run.clock)
            .unwrap();
        let request = member.next_request();
        let raw = serde_json::to_vec(&result).unwrap();
        let answer = member.answer(&request, page(7, 0, raw.len(), &raw));
        match member.receive(&answer) {
            Ok(()) => member.assert_component_failure(),
            Err(failure) => member.assert_protocol_failure(failure),
        }
        let OperationCompletion::State { completion, .. } = member
            .run
            .context
            .read
            .as_ref()
            .unwrap()
            .rejected_for_test()
            .unwrap()
        else {
            panic!("retained state answer")
        };
        assert_eq!(
            serde_json::to_value(&completion.answer).unwrap(),
            serde_json::to_value(&answer).unwrap()
        );
        assert!(member.records(Kind::Recall).is_empty());
        assert_eq!(
            member.requests.len(),
            1,
            "malformed content must not issue Release"
        );
    }
}

struct FailedReadClock;
impl PumpIo for FailedReadClock {
    fn now(&mut self) -> std::io::Result<u64> {
        Err(std::io::Error::other("read fixture clock failed"))
    }
    fn wait(&mut self, _: &[WaitInterest<'_>], _: Option<u64>) -> std::io::Result<WaitOutcome> {
        panic!("local failure path");
    }
}

#[test]
fn failed_read_closes_only_an_unreleased_exchange() {
    for after_release in [false, true] {
        let mut member = Member::new();
        if after_release {
            released(&mut member);
        } else {
            let request = member.begin();
            member.respond(&request, page(7, 0, EMPTY.len(), EMPTY));
            let _release = member.next_request();
        }
        let failure = member
            .run
            .context
            .advance_read(&mut ports(&mut FailedReadClock))
            .unwrap_err();
        assert!(matches!(
            failure,
            RunFailure::Flow {
                reason: FlowHaltReason::TraceFailed,
                ..
            }
        ));
        assert!(member.run.context.read.is_some());
        assert!(
            member
                .run
                .context
                .halt()
                .unwrap()
                .secondary()
                .iter()
                .all(|secondary| secondary.reason != FlowHaltReason::CarryFailed)
        );
        member.run.state.set_nonblocking(true).unwrap();
        let result = member.run.state.read(&mut [0]);
        if after_release {
            assert_eq!(result.unwrap_err().kind(), std::io::ErrorKind::WouldBlock);
        } else {
            assert_eq!(result.unwrap(), 0);
        }
    }
}

#[test]
fn recall_recorded_boundary_cannot_extend_the_original_read_deadline() {
    let mut member = Member::new();
    released(&mut member);
    let bound = member.bound();
    let now = member.now();
    let mut clock = RecallClock {
        before: now,
        expired: bound.deadline_ns(),
        remaining: 3,
    };
    assert!(matches!(
        member.run.context.advance_read(&mut ports(&mut clock)),
        Err(RunFailure::Flow {
            reason: FlowHaltReason::ControlBound,
            ..
        })
    ));
    assert_eq!(member.records(Kind::Recall).len(), 1);
    assert!(member.run.context.take_read(&mut clock).is_err());
    assert_eq!(member.requests.len(), 2);
}

#[test]
fn cancelled_recorded_read_cannot_reacquire_consumption_or_repeat_recall() {
    let mut member = Member::new();
    released(&mut member);
    assert_eq!(member.advance().unwrap(), ReadProgress::Complete);
    member
        .run
        .context
        .cancel_read(&mut member.run.clock)
        .unwrap();
    let ReadCompletion::Cancelled(cancelled) = take(&mut member) else {
        panic!("late cancellation did not revoke consumption");
    };
    assert!(cancelled.recall_sequence().is_some());
    assert!(cancelled.refusal().is_none());
    assert_eq!(member.records(Kind::Recall).len(), 1);
    member.no_result();
}
