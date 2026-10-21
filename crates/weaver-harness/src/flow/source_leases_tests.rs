//! Ledger-level custody tests. Real recorder/socket provenance is exercised by
//! the context integration suite, not by these explicit proof fixtures.
use super::*;
use std::sync::Arc;
use weaver_trace::Sequence;
use weaver_trace::serving::{SourceBlock, StatementCopy};
use weaver_types::state_flow::*;

fn ledger() -> Ledger {
    Ledger::fixture("run".into(), "session".into())
}

fn id(sequence: u64, block: u64) -> SourceBlockId {
    SourceBlockId {
        run: "run".into(),
        sequence: sequence.into(),
        block: block.into(),
    }
}

fn source(sequence: u64, kind: &str, channel: &str, origin: &str) -> Arc<str> {
    serde_json::json!({
        "session":"session", "run":"run", "turn":"turn", "sequence":sequence.to_string(),
        "kind":kind, "channel":channel, "origin":origin, "subsystem":"harness",
        "wall_ms":1, "monotonic_ns":"7", "causal_parent":"1"
    })
    .to_string()
    .into()
}

fn statement(sequence: u64, ordinals: &[u64]) -> AdmittedCopy {
    AdmittedCopy::Statement(StatementCopy {
        source_envelope: source(sequence, "message.assistant", "output", "model"),
        partition_rule_id: "statement".into(),
        blocks: ordinals
            .iter()
            .map(|index| SelectedBlock {
                source: SourceBlock {
                    run: "run".into(),
                    sequence: Sequence(sequence),
                    block: *index,
                },
                source_path: vec![PathPart::Key("content".into()), PathPart::Index(*index)],
                raw: Arc::from("{ \"type\" : \"text\", \"text\" : \"Read caf\\u00e9.\" }"),
            })
            .collect(),
        context: None,
    })
}

fn binding() -> Binding {
    Binding {
        statement_span: Span {
            start: 0.into(),
            end: 11.into(),
        },
        verb: "read".into(),
        verb_span: Span {
            start: 0.into(),
            end: 4.into(),
        },
        condition: None,
        condition_span: None,
        nouns: vec![NounBinding {
            value: "caf\u{e9}".into(),
            span: Span {
                start: 5.into(),
                end: 10.into(),
            },
        }],
    }
}

fn check(
    ledger: &mut Ledger,
    source: SourceBlockId,
    request_id: u64,
    accepted: bool,
) -> Classified {
    let request = Ste100Request {
        request_id: request_id.into(),
        target: Ste100Target::Source(source.clone()),
        text: "Read caf\u{e9}.".into(),
        scheme_id: "fixture-read/1".into(),
    };
    ledger.classification_submitted(&source, &request).unwrap();
    ledger
        .classify_recorded(&Ste100Reply {
            request_id: request.request_id,
            target: request.target,
            scheme_id: request.scheme_id,
            result: if accepted {
                Ste100Result::Accepted {
                    bindings: vec![binding()],
                }
            } else {
                Ste100Result::NotSte100 {
                    code: Ste100DefectCode::NonSte100,
                    detail: "fixture defect".into(),
                    spans: vec![],
                }
            },
        })
        .unwrap()
}

fn rejected_proof(source: SourceBlockId) -> RecordedTerminal {
    RecordedTerminal::for_test_phase(
        source,
        weaver_trace::serving::payloads::FlowPhase::CorrectlyRejected,
    )
}

fn budget() -> RetentionBudget {
    RetentionBudget::new(FlowBacklog {
        max_pending_blocks: 20.into(),
        max_pending_bytes: 50_000.into(),
        lag_alarm_turns: 2.into(),
        measurement_ref: "fixture".into(),
    })
    .unwrap()
}

fn admit(ledger: &mut Ledger, budget: &mut RetentionBudget, copy: AdmittedCopy) -> InstallReport {
    let mut inbox = super::super::intake::FlowInbox::new();
    assert!(matches!(
        inbox.accept(copy, budget),
        super::super::intake::IntakeOutcome::Accepted
    ));
    ledger
        .install(inbox.pop_front(budget).unwrap(), 10, None)
        .unwrap()
}

fn common() -> Common {
    Common {
        session_id: "session".into(),
        run_id: "run".into(),
        request_id: 2.into(),
    }
}

fn observation() -> StateOperationObservation {
    StateOperationObservation {
        operation_id: 2.into(),
        parent_id: None,
        request_id: 2.into(),
        operation: StateOperation::File,
        began_ns: 15.into(),
        ended_ns: 20.into(),
        phases: vec![
            OperationPhase {
                name: PhaseName::Validate,
                began_ns: 15.into(),
                ended_ns: 16.into(),
            },
            OperationPhase {
                name: PhaseName::Encode,
                began_ns: 19.into(),
                ended_ns: 20.into(),
            },
        ],
        resource: OperationResource {
            cpu_ns: None,
            copied_bytes: Some(1.into()),
            held_bytes: Some(2.into()),
            unavailable: vec![ResourceUnavailable {
                field: ResourceField::CpuNs,
                reason: ObservationUnavailable::NotSampled,
            }],
        },
        disk: OperationDisk {
            enabled: None,
            before: None,
            after: None,
            unavailable_reason: Some(ObservationUnavailable::NotSampled),
        },
    }
}

fn commit(ledger: &mut Ledger, source: &SourceBlockId, request_id: u64) -> StateFrame {
    let operation = ledger.next_file().unwrap().operation();
    let mut exchange = common();
    exchange.request_id = request_id.into();
    ledger.begin_file(source, &exchange).unwrap();
    ledger
        .observe_progress(
            &ExchangeProgress {
                common: exchange.clone(),
                operation,
                source: Some(source.clone()),
                bytes_sent: 100,
                frame_bytes: 100,
                commit: CommitEvidence::Committed,
            },
            Some(11),
        )
        .unwrap();
    let mut observed = observation();
    observed.operation = operation;
    observed.request_id = request_id.into();
    observed.operation_id = request_id.into();
    let answer = StateFrame::Filed {
        common: exchange,
        block_id: source.clone(),
        observation: observed,
    };
    ledger.receipt_recorded(&answer).unwrap();
    answer
}

#[test]
fn lease_transfers_one_exact_terminal_without_waiting_for_its_sibling() {
    let mut ledger = ledger();
    let mut budget = budget();
    admit(&mut ledger, &mut budget, statement(2, &[0, 3]));
    let lease = ledger
        .register_source_lease(vec![id(2, 0), id(2, 3)])
        .unwrap();
    check(&mut ledger, id(2, 0), 1, true);
    let LedgerFile::Statement(file) = &ledger.entries[0].file else {
        panic!("statement")
    };
    let raw_pointer = file.raw_block.get().as_ptr();
    let file_json = serde_json::to_string(file).unwrap();
    assert!(
        ledger
            .take_source_receipt(&lease, &id(2, 0))
            .unwrap()
            .is_none()
    );
    let answer = commit(&mut ledger, &id(2, 0), 9);
    assert!(
        ledger
            .take_source_receipt(&lease, &id(2, 0))
            .unwrap()
            .is_none()
    );
    assert!(
        ledger
            .terminal_recorded(rejected_proof(id(2, 0)), &mut budget)
            .is_err()
    );
    let terminal = ledger
        .terminal_recorded(RecordedTerminal::for_test(id(2, 0)), &mut budget)
        .unwrap();
    assert_eq!(
        ledger.source_lease_status(&lease, &id(2, 0)).unwrap(),
        LeaseSlotStatus::Ready
    );
    assert_eq!(
        ledger.source_lease_status(&lease, &id(2, 3)).unwrap(),
        LeaseSlotStatus::Pending
    );
    assert_eq!(
        ledger.source_lease_retained(&lease).unwrap(),
        (1, terminal.bytes)
    );
    let Some(SourceDisposition::Committed(committed)) =
        ledger.take_source_receipt(&lease, &id(2, 0)).unwrap()
    else {
        panic!("exact committed receipt")
    };
    let LedgerFile::Statement(file) = committed.file() else {
        panic!("statement")
    };
    assert_eq!(file.raw_block.get().as_ptr(), raw_pointer);
    assert_eq!(serde_json::to_string(file).unwrap(), file_json);
    assert_eq!(
        serde_json::to_string(committed.receipt()).unwrap(),
        serde_json::to_string(&answer).unwrap()
    );
    assert_eq!(committed.terminal(), &terminal);
    assert_eq!(
        ledger.source_lease_status(&lease, &id(2, 0)).unwrap(),
        LeaseSlotStatus::Taken
    );
    assert!(ledger.take_source_receipt(&lease, &id(2, 0)).is_err());
    assert_eq!(ledger.source_lease_retained(&lease).unwrap(), (0, 0));
    assert_eq!(ledger.facts().count(), 1);
}

#[test]
fn lease_retains_correct_rejection_without_a_filed_capability() {
    let mut ledger = ledger();
    let mut budget = budget();
    admit(&mut ledger, &mut budget, statement(2, &[0]));
    let lease = ledger.register_source_lease(vec![id(2, 0)]).unwrap();
    check(&mut ledger, id(2, 0), 1, false);
    assert!(
        ledger
            .terminal_recorded(RecordedTerminal::for_test(id(2, 0)), &mut budget)
            .is_err()
    );
    assert!(
        ledger
            .take_source_receipt(&lease, &id(2, 0))
            .unwrap()
            .is_none()
    );
    ledger
        .terminal_recorded(rejected_proof(id(2, 0)), &mut budget)
        .unwrap();
    let Some(SourceDisposition::CorrectlyRejected(rejected)) =
        ledger.take_source_receipt(&lease, &id(2, 0)).unwrap()
    else {
        panic!("correct rejection is not a committed source")
    };
    assert!(matches!(
        rejected.reply().result,
        Ste100Result::NotSte100 { .. }
    ));
    assert_eq!(rejected.source(), &id(2, 0));
    assert!(ledger.next_file().is_none());
}

#[test]
fn disjoint_leases_and_bulk_subscription_coexist_but_overlap_is_atomic_refusal() {
    let mut ledger = ledger();
    let mut budget = budget();
    admit(&mut ledger, &mut budget, statement(2, &[0, 3, 5]));
    let first = ledger.register_source_lease(vec![id(2, 0)]).unwrap();
    let second = ledger.register_source_lease(vec![id(2, 3)]).unwrap();
    ledger.register_receipts(vec![id(2, 5)]).unwrap();
    for source in [id(2, 0), id(2, 3), id(2, 5)] {
        let refused = vec![source];
        assert_eq!(
            ledger
                .register_source_lease(refused.clone())
                .unwrap_err()
                .sources,
            refused
        );
    }
    assert_eq!(ledger.source_lease_count(), 2);
    ledger.cancel_receipts();
    assert!(ledger.register_receipts(vec![id(2, 3)]).is_err());
    assert!(ledger.receipts().is_none());
    ledger.release_source_lease(first).unwrap();
    ledger.register_receipts(vec![id(2, 0)]).unwrap();
    ledger.release_source_lease(second).unwrap();
    assert!(!ledger.has_source_leases());
    assert_eq!(ledger.facts().count(), 3);
}

#[test]
fn lease_registration_refuses_foreign_unknown_duplicate_order_and_disposed_history() {
    let mut ledger = ledger();
    let mut budget = budget();
    admit(&mut ledger, &mut budget, statement(2, &[0, 3]));
    let mut foreign = id(2, 0);
    foreign.run = "foreign".into();
    for sources in [
        vec![foreign],
        vec![id(2, 1)],
        vec![id(2, 0), id(2, 0)],
        vec![id(2, 3), id(2, 0)],
    ] {
        assert_eq!(
            ledger
                .register_source_lease(sources.clone())
                .unwrap_err()
                .sources,
            sources
        );
        assert!(!ledger.has_source_leases());
    }
    check(&mut ledger, id(2, 0), 1, false);
    ledger
        .terminal_recorded(rejected_proof(id(2, 0)), &mut budget)
        .unwrap();
    assert!(ledger.barrier(&[id(2, 0)]).unwrap());
    assert!(ledger.register_source_lease(vec![id(2, 0)]).is_err());
    ledger.entries[0].raw_bytes = u64::MAX;
    ledger.entries[0].context_bytes = 1;
    assert!(ledger.register_source_lease(vec![id(2, 3)]).is_err());
    assert!(!ledger.has_source_leases());
}

#[test]
fn lease_token_is_bound_to_one_ledger_even_for_identical_run_and_first_counter() {
    let mut first = ledger();
    let mut second = ledger();
    let mut first_budget = budget();
    let mut second_budget = budget();
    admit(&mut first, &mut first_budget, statement(2, &[0]));
    admit(&mut second, &mut second_budget, statement(2, &[0]));
    let first_lease = first.register_source_lease(vec![id(2, 0)]).unwrap();
    let second_lease = second.register_source_lease(vec![id(2, 0)]).unwrap();
    assert!(second.source_lease_status(&first_lease, &id(2, 0)).is_err());
    assert!(second.take_source_receipt(&first_lease, &id(2, 0)).is_err());
    assert!(
        second
            .release_source_receipt(&first_lease, &id(2, 0))
            .is_err()
    );
    let (first_lease, _) = second.release_source_lease(first_lease).unwrap_err();
    assert_eq!(
        second
            .source_lease_status(&second_lease, &id(2, 0))
            .unwrap(),
        LeaseSlotStatus::Pending
    );
    first.release_source_lease(first_lease).unwrap();
    second.release_source_lease(second_lease).unwrap();
}

#[test]
fn releasing_a_slot_does_not_release_owed_filing_and_allows_new_explicit_interest() {
    let mut ledger = ledger();
    let mut budget = budget();
    admit(&mut ledger, &mut budget, statement(2, &[0, 3]));
    let charged = budget.gauges();
    let lease = ledger
        .register_source_lease(vec![id(2, 0), id(2, 3)])
        .unwrap();
    ledger.release_source_receipt(&lease, &id(2, 0)).unwrap();
    assert_eq!(
        ledger.source_lease_status(&lease, &id(2, 0)).unwrap(),
        LeaseSlotStatus::Released
    );
    assert!(ledger.take_source_receipt(&lease, &id(2, 0)).is_err());
    assert!(ledger.release_source_receipt(&lease, &id(2, 0)).is_err());
    assert_eq!(budget.gauges(), charged);
    assert_eq!(ledger.facts().count(), 2);
    let replacement = ledger.register_source_lease(vec![id(2, 0)]).unwrap();
    check(&mut ledger, id(2, 0), 1, true);
    commit(&mut ledger, &id(2, 0), 2);
    ledger
        .terminal_recorded(RecordedTerminal::for_test(id(2, 0)), &mut budget)
        .unwrap();
    assert!(
        ledger
            .take_source_receipt(&replacement, &id(2, 0))
            .unwrap()
            .is_some()
    );
    assert!(ledger.take_source_receipt(&lease, &id(2, 0)).is_err());
    assert_eq!(
        ledger.source_lease_status(&lease, &id(2, 3)).unwrap(),
        LeaseSlotStatus::Pending
    );
}

#[test]
fn release_whole_lease_discards_terminal_fact_but_preserves_pending_obligation() {
    let mut ledger = ledger();
    let mut budget = budget();
    admit(&mut ledger, &mut budget, statement(2, &[0, 3]));
    let lease = ledger
        .register_source_lease(vec![id(2, 0), id(2, 3)])
        .unwrap();
    check(&mut ledger, id(2, 0), 1, false);
    ledger
        .terminal_recorded(rejected_proof(id(2, 0)), &mut budget)
        .unwrap();
    let charged = budget.gauges();
    assert_eq!(ledger.source_lease_retained(&lease).unwrap().0, 1);
    ledger.release_source_lease(lease).unwrap();
    assert_eq!(budget.gauges(), charged);
    assert_eq!(ledger.facts().count(), 1);
    check(&mut ledger, id(2, 3), 2, false);
    ledger
        .terminal_recorded(rejected_proof(id(2, 3)), &mut budget)
        .unwrap();
    assert_eq!(budget.gauges(), (0, 0));
    assert!(!ledger.has_source_leases());
}

#[test]
fn frozen_ledger_keeps_evidence_but_cannot_issue_a_lease_or_terminal_grant() {
    let mut ledger = ledger();
    let mut budget = budget();
    admit(&mut ledger, &mut budget, statement(2, &[0]));
    let lease = ledger.register_source_lease(vec![id(2, 0)]).unwrap();
    check(&mut ledger, id(2, 0), 1, true);
    commit(&mut ledger, &id(2, 0), 2);
    ledger
        .terminal_recorded(RecordedTerminal::for_test(id(2, 0)), &mut budget)
        .unwrap();
    ledger.freeze();
    assert!(ledger.take_source_receipt(&lease, &id(2, 0)).is_err());
    assert!(ledger.source_lease_status(&lease, &id(2, 0)).is_err());
    assert!(ledger.register_source_lease(vec![]).is_err());
    assert_eq!(ledger.source_lease_retained(&lease).unwrap().0, 1);
    ledger.release_source_lease(lease).unwrap();
    assert!(!ledger.has_source_leases());
}

#[test]
fn empty_lease_and_outside_source_never_invent_commit_evidence() {
    let mut ledger = ledger();
    let lease = ledger.register_source_lease(vec![]).unwrap();
    assert_eq!(ledger.source_lease_sources(&lease).unwrap().count(), 0);
    assert!(ledger.take_source_receipt(&lease, &id(2, 0)).is_err());
    assert!(ledger.release_source_receipt(&lease, &id(2, 0)).is_err());
    assert_eq!(ledger.source_lease_retained(&lease).unwrap(), (0, 0));
    assert!(ledger.barrier(&[]).unwrap());
    assert!(ledger.has_source_leases());
    ledger.release_source_lease(lease).unwrap();
    assert!(!ledger.has_source_leases());
}

#[test]
fn source_lease_cannot_clone_or_default() {
    trait NotClone<Marker> {
        fn check() {}
    }
    impl<T: ?Sized> NotClone<()> for T {}
    impl<T: Clone> NotClone<u8> for T {}
    let _ = <SourceLease as NotClone<_>>::check;
    trait NotDefault<Marker> {
        fn check() {}
    }
    impl<T: ?Sized> NotDefault<()> for T {}
    impl<T: Default> NotDefault<u8> for T {}
    let _ = <SourceLease as NotDefault<_>>::check;
}

#[test]
fn dropped_lease_never_drops_debt_and_terminal_progress_discards_abandoned_custody() {
    let mut ledger = ledger();
    let mut budget = budget();
    admit(&mut ledger, &mut budget, statement(2, &[0]));
    let charged = budget.gauges();
    let lease = ledger.register_source_lease(vec![id(2, 0)]).unwrap();
    drop(lease);
    assert!(!ledger.has_source_leases());
    assert_eq!(budget.gauges(), charged);
    assert_eq!(ledger.facts().count(), 1);
    check(&mut ledger, id(2, 0), 1, true);
    commit(&mut ledger, &id(2, 0), 2);
    ledger
        .terminal_recorded(RecordedTerminal::for_test(id(2, 0)), &mut budget)
        .unwrap();
    assert_eq!(budget.gauges(), (0, 0));
    assert!(!ledger.has_source_leases());
    assert!(ledger.register_source_lease(vec![id(2, 0)]).is_err());
}

#[test]
fn dropped_ready_lease_is_abandoned_before_the_next_generation_registers() {
    let mut ledger = ledger();
    let mut budget = budget();
    admit(&mut ledger, &mut budget, statement(2, &[0]));
    let lease = ledger.register_source_lease(vec![id(2, 0)]).unwrap();
    check(&mut ledger, id(2, 0), 1, true);
    commit(&mut ledger, &id(2, 0), 2);
    ledger
        .terminal_recorded(RecordedTerminal::for_test(id(2, 0)), &mut budget)
        .unwrap();
    assert_eq!(ledger.source_lease_retained(&lease).unwrap().0, 1);
    drop(lease);
    assert_eq!(ledger.source_lease_count(), 0);
    admit(&mut ledger, &mut budget, statement(4, &[0]));
    let next = ledger.register_source_lease(vec![id(4, 0)]).unwrap();
    assert_eq!(ledger.source_lease_count(), 1);
    assert_eq!(ledger.source_lease_retained(&next).unwrap(), (0, 0));
    assert_eq!(ledger.facts().count(), 1);
}
