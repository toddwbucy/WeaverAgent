//! Generation receipt leases use the real recorder and state socket owners.
//! Scripted member replies demonstrate custody, not SQLite/model qualification.

use super::output_tests::{OutputFixture, original, reply, text};
use super::*;
use crate::flow::author::AuthoredSource;
use crate::flow::coordinator::{
    CheckedGeneration, OriginalRequests, RequestScheme, TransportSource, tests::operation,
};
use crate::flow::ledger::{LeaseSlotStatus, LedgerFile, SourceDisposition, SourceLease};
use crate::flow::pump::LifecyclePorts;
use sha2::{Digest as _, Sha256};
use std::io::Write;
use weaver_trace::{Subsystem, TurnRef};
use weaver_traits::{ContentBlock, Message, Role, ToolCall};
use weaver_types::state_flow as s;

fn source(generation: &CheckedGeneration, index: u64) -> s::SourceBlockId {
    s::SourceBlockId {
        block: index.into(),
        ..generation.source().block_id.clone()
    }
}

fn event(value: &str) -> AuthoredEvent {
    AuthoredEvent {
        kind: Kind::MessageAssistant,
        subsystem: Subsystem::Harness,
        turn: Some(TurnRef("output-turn".into())),
        source: AuthoredSource::Model,
        causal_parent: None,
        payload: Some(Payload::Message(
            serde_json::value::to_raw_value(&Message {
                role: Role::Assistant,
                content: vec![text(value)],
            })
            .unwrap(),
        )),
    }
}

fn take_generation(fx: &mut OutputFixture) -> CheckedGeneration {
    fx.input
        .run
        .context
        .take_output(&mut fx.input.run.clock)
        .unwrap()
        .unwrap()
}

fn take_receipt(
    fx: &mut OutputFixture,
    lease: &SourceLease,
    source: &s::SourceBlockId,
) -> Option<SourceDisposition> {
    fx.input
        .run
        .context
        .take_source_receipt(lease, source, &mut fx.input.run.clock)
        .unwrap()
}

fn background(fx: &mut OutputFixture) {
    fx.input
        .run
        .context
        .reduce_flow(&mut fx.input.run.clock)
        .unwrap();
    fx.member(false);
}

fn await_file(fx: &mut OutputFixture, count: usize) {
    for _ in 0..128 {
        background(fx);
        if fx.file_requests.len() >= count {
            return;
        }
    }
    panic!("the expected filing never reached the real member socket");
}

/// Answer a previously withheld File. Receiving its bytes alone cannot satisfy
/// the lease, and this helper still leaves its measurement and terminal pending.
fn answer_file(fx: &mut OutputFixture, index: usize) {
    let request = &fx.file_requests[index];
    let s::StateFrame::File { filing, .. } = request else {
        panic!("statement filing expected");
    };
    let began = fx.input.run.clock.now().unwrap() + 1;
    let response = s::StateFrame::Filed {
        common: request.common().clone(),
        block_id: filing.block_id.clone(),
        observation: operation(request, fx.operation, began),
    };
    fx.operation += 1;
    fx.input.run.clock.advance_to(began + 1).unwrap();
    fx.input
        .run
        .state
        .write_all(&s::encode_state_line(&response, s::FrameDirection::Response).unwrap())
        .unwrap();
    fx.transport(TransportSource::State);
}

fn await_disposed(fx: &mut OutputFixture, source: &s::SourceBlockId) {
    for _ in 0..128 {
        background(fx);
        if !fx
            .input
            .run
            .context
            .flow
            .ledger()
            .facts()
            .any(|fact| fact.source == source)
        {
            return;
        }
    }
    panic!("the exact source never reached its recorded terminal disposition");
}

fn assert_committed(
    outcome: SourceDisposition,
    expected: &s::SourceBlockId,
    text: &str,
    request: &s::StateFrame,
) {
    let SourceDisposition::Committed(committed) = outcome else {
        panic!("expected a committed filing, not a conformance rejection");
    };
    assert_eq!(committed.source(), expected);
    assert_eq!(committed.common(), request.common());
    let LedgerFile::Statement(filing) = committed.file() else {
        panic!("expected the original statement filing");
    };
    assert_eq!(filing.text, text);
    assert_eq!(&filing.block_id, expected);
    assert_eq!(committed.receipt().common(), request.common());
}

#[test]
fn an_early_commit_survives_until_the_whole_generation_returns() {
    let mut fx = OutputFixture::new();
    fx.begin(original(vec![text("Read early."), text("Read later.")]));
    let first = fx.request();
    fx.input.reply(&reply(&first, false));
    let later = fx.request();
    assert_eq!(later.text, "Read later.");
    let s::Ste100Target::Source(first_source) = first.target else {
        panic!("model source expected");
    };
    await_file(&mut fx, 1);
    answer_file(&mut fx, 0);
    await_disposed(&mut fx, &first_source);
    assert!(
        fx.input
            .run
            .context
            .take_output(&mut fx.input.run.clock)
            .unwrap()
            .is_none(),
        "the later check remains owed"
    );
    fx.input.reply(&reply(&later, false));
    fx.complete(None, false);
    let mut checked = take_generation(&mut fx);
    let lease = checked.take_receipt_lease().unwrap();
    let receipt = take_receipt(&mut fx, &lease, &first_source).unwrap();
    assert_committed(receipt, &first_source, "Read early.", &fx.file_requests[0]);
    assert_eq!(fx.requests.len(), 2);
    assert!(fx.input.run.context.failure().is_none());
}

#[test]
fn the_returned_generation_can_take_its_later_committed_receipt() {
    let mut fx = OutputFixture::new();
    fx.begin(original(vec![text("Read delayed.")]));
    fx.complete(None, false);
    let mut checked = take_generation(&mut fx);
    let source = source(&checked, 0);
    let lease = checked.take_receipt_lease().unwrap();
    assert!(take_receipt(&mut fx, &lease, &source).is_none());
    await_file(&mut fx, 1);
    answer_file(&mut fx, 0);
    assert!(
        take_receipt(&mut fx, &lease, &source).is_none(),
        "a member answer alone is not the recorded terminal grant"
    );
    await_disposed(&mut fx, &source);
    let receipt = take_receipt(&mut fx, &lease, &source).unwrap();
    assert_committed(receipt, &source, "Read delayed.", &fx.file_requests[0]);
    assert_eq!(fx.requests.len(), 1);
}

#[test]
fn a_pending_ordinary_sibling_does_not_block_an_available_source_receipt() {
    let mut fx = OutputFixture::new();
    fx.begin(original(vec![text("Read request."), text("Read sibling.")]));
    fx.complete(None, false);
    let mut checked = take_generation(&mut fx);
    let request = source(&checked, 0);
    let sibling = source(&checked, 1);
    let lease = checked.take_receipt_lease().unwrap();
    await_file(&mut fx, 1);
    answer_file(&mut fx, 0);
    await_disposed(&mut fx, &request);
    assert!(take_receipt(&mut fx, &lease, &sibling).is_none());
    let receipt = take_receipt(&mut fx, &lease, &request).unwrap();
    assert_committed(receipt, &request, "Read request.", &fx.file_requests[0]);
    assert!(
        fx.input
            .run
            .context
            .flow
            .ledger()
            .facts()
            .any(|fact| fact.source == &sibling)
    );
    assert_eq!(fx.requests.len(), 2);
}

#[test]
fn receipt_leases_from_two_generations_remain_independent() {
    let mut fx = OutputFixture::new();
    fx.begin(original(vec![text("Read first.")]));
    fx.complete(None, false);
    let mut first = take_generation(&mut fx);
    let first_source = source(&first, 0);
    let first_lease = first.take_receipt_lease().unwrap();
    fx.begin(original(vec![text("Read second.")]));
    fx.complete(None, false);
    let mut second = take_generation(&mut fx);
    let second_source = source(&second, 0);
    let second_lease = second.take_receipt_lease().unwrap();
    assert!(take_receipt(&mut fx, &first_lease, &first_source).is_none());
    assert!(take_receipt(&mut fx, &second_lease, &second_source).is_none());
    await_file(&mut fx, 1);
    answer_file(&mut fx, 0);
    await_disposed(&mut fx, &first_source);
    let receipt = take_receipt(&mut fx, &first_lease, &first_source).unwrap();
    assert_committed(receipt, &first_source, "Read first.", &fx.file_requests[0]);
    assert!(take_receipt(&mut fx, &second_lease, &second_source).is_none());
    await_file(&mut fx, 2);
    answer_file(&mut fx, 1);
    await_disposed(&mut fx, &second_source);
    let receipt = take_receipt(&mut fx, &second_lease, &second_source).unwrap();
    assert_committed(
        receipt,
        &second_source,
        "Read second.",
        &fx.file_requests[1],
    );
    assert!(fx.input.run.context.failure().is_none());
}

#[test]
fn releasing_a_generation_lease_preserves_the_underlying_owed_filing() {
    let mut fx = OutputFixture::new();
    fx.begin(original(vec![text("Read retained.")]));
    fx.complete(None, false);
    let mut checked = take_generation(&mut fx);
    let source = source(&checked, 0);
    let lease = checked.take_receipt_lease().unwrap();
    fx.input.run.context.release_source_lease(lease).unwrap();
    assert!(
        fx.input
            .run
            .context
            .flow
            .ledger()
            .facts()
            .any(|fact| fact.source == &source)
    );
    await_file(&mut fx, 1);
    answer_file(&mut fx, 0);
    await_disposed(&mut fx, &source);
    assert_eq!(fx.file_requests.len(), 1);
    assert!(fx.input.run.context.flow.ledger().facts().next().is_none());
    assert!(fx.input.run.context.failure().is_none());
}

#[test]
fn another_context_cannot_use_a_lease_even_with_equal_wire_identifiers() {
    let mut first = OutputFixture::new();
    let mut other = OutputFixture::new();
    first.begin(original(vec![text("Read exact.")]));
    first.complete(None, false);
    other.begin(original(vec![text("Read exact.")]));
    other.complete(None, false);
    let mut owned = take_generation(&mut first);
    let other_checked = take_generation(&mut other);
    let other_source = source(&other_checked, 0);
    let source = source(&owned, 0);
    assert_eq!(source, other_source);
    let lease = owned.take_receipt_lease().unwrap();
    let refused =
        other
            .input
            .run
            .context
            .take_source_receipt(&lease, &source, &mut other.input.run.clock);
    assert!(refused.is_err());
    assert!(other.input.run.context.failure().is_none());
    let (lease, _) = other
        .input
        .run
        .context
        .release_source_lease(lease)
        .unwrap_err();
    await_file(&mut first, 1);
    answer_file(&mut first, 0);
    await_disposed(&mut first, &source);
    assert!(take_receipt(&mut first, &lease, &source).is_some());
}

#[test]
fn each_source_receipt_transfers_once_and_an_unknown_source_cannot_consume_it() {
    let mut fx = OutputFixture::new();
    fx.begin(original(vec![text("Read once.")]));
    fx.complete(None, false);
    let mut checked = take_generation(&mut fx);
    let source = source(&checked, 0);
    let lease = checked.take_receipt_lease().unwrap();
    let unknown = s::SourceBlockId {
        block: 99.into(),
        ..source.clone()
    };
    assert!(
        fx.input
            .run
            .context
            .take_source_receipt(&lease, &unknown, &mut fx.input.run.clock,)
            .is_err()
    );
    assert_eq!(
        fx.input
            .run
            .context
            .source_receipt_status(&lease, &source, &mut fx.input.run.clock,)
            .unwrap(),
        LeaseSlotStatus::Pending
    );
    await_file(&mut fx, 1);
    answer_file(&mut fx, 0);
    await_disposed(&mut fx, &source);
    assert_eq!(
        fx.input
            .run
            .context
            .source_receipt_status(&lease, &source, &mut fx.input.run.clock,)
            .unwrap(),
        LeaseSlotStatus::Ready
    );
    let receipt = take_receipt(&mut fx, &lease, &source).unwrap();
    assert_committed(receipt, &source, "Read once.", &fx.file_requests[0]);
    assert_eq!(
        fx.input
            .run
            .context
            .source_receipt_status(&lease, &source, &mut fx.input.run.clock,)
            .unwrap(),
        LeaseSlotStatus::Taken
    );
    assert!(
        fx.input
            .run
            .context
            .take_source_receipt(&lease, &source, &mut fx.input.run.clock,)
            .is_err()
    );
    assert!(fx.input.run.context.failure().is_none());
}

#[test]
fn releasing_one_source_keeps_other_source_receipts_and_both_filings() {
    let mut fx = OutputFixture::new();
    fx.begin(original(vec![text("Read released."), text("Read kept.")]));
    fx.complete(None, false);
    let mut checked = take_generation(&mut fx);
    let released = source(&checked, 0);
    let kept = source(&checked, 1);
    let lease = checked.take_receipt_lease().unwrap();
    fx.input
        .run
        .context
        .release_source_receipt(&lease, &released)
        .unwrap();
    assert_eq!(
        fx.input
            .run
            .context
            .source_receipt_status(&lease, &released, &mut fx.input.run.clock,)
            .unwrap(),
        LeaseSlotStatus::Released
    );
    assert!(
        fx.input
            .run
            .context
            .take_source_receipt(&lease, &released, &mut fx.input.run.clock,)
            .is_err()
    );
    await_file(&mut fx, 1);
    answer_file(&mut fx, 0);
    await_disposed(&mut fx, &released);
    await_file(&mut fx, 2);
    answer_file(&mut fx, 1);
    await_disposed(&mut fx, &kept);
    let receipt = take_receipt(&mut fx, &lease, &kept).unwrap();
    assert_committed(receipt, &kept, "Read kept.", &fx.file_requests[1]);
    assert_eq!(fx.file_requests.len(), 2);
    assert!(fx.input.run.context.failure().is_none());
}

#[test]
fn a_halt_does_not_release_an_already_recorded_receipt() {
    let mut fx = OutputFixture::new();
    fx.begin(original(vec![text("Read frozen.")]));
    fx.complete(None, false);
    let mut checked = take_generation(&mut fx);
    let source = source(&checked, 0);
    let lease = checked.take_receipt_lease().unwrap();
    await_file(&mut fx, 1);
    answer_file(&mut fx, 0);
    await_disposed(&mut fx, &source);
    fx.input
        .run
        .clock
        .advance_to(fx.bound.deadline_ns())
        .unwrap();
    assert!(matches!(
        fx.input.run.context.reduce_flow(&mut fx.input.run.clock),
        Err(RunFailure::Flow {
            reason: FlowHaltReason::DecisionBound,
            ..
        })
    ));
    assert!(
        fx.input
            .run
            .context
            .take_source_receipt(&lease, &source, &mut fx.input.run.clock,)
            .is_err()
    );
}

#[test]
fn deferred_output_copy_registers_its_lease_before_any_disposition_can_retire() {
    let mut declaration = super::tests::config();
    declaration.backlog.max_pending_blocks = 1.into();
    let mut fx = OutputFixture::configured(declaration);
    fx.input.run.author(event("Read older.")).unwrap();
    fx.begin(original(vec![text("Read deferred.")]));
    assert_eq!(fx.advance().unwrap(), OutputProgress::Progress);
    assert!(fx.input.run.context.budget.waiting_copy().is_some());
    assert_eq!(fx.input.run.context.flow.ledger().source_lease_count(), 0);
    let older = fx.request();
    assert_eq!(older.text, "Read older.");
    fx.input.reply(&reply(&older, false));
    let s::Ste100Target::Source(older_source) = older.target else {
        panic!("selected source expected");
    };
    await_file(&mut fx, 1);
    answer_file(&mut fx, 0);
    await_disposed(&mut fx, &older_source);
    let deferred = fx.request();
    assert_eq!(deferred.text, "Read deferred.");
    assert_eq!(fx.input.run.context.flow.ledger().source_lease_count(), 1);
    fx.input.reply(&reply(&deferred, false));
    fx.complete(None, false);
    let mut checked = take_generation(&mut fx);
    let source = source(&checked, 0);
    let lease = checked.take_receipt_lease().unwrap();
    await_file(&mut fx, 2);
    answer_file(&mut fx, 1);
    await_disposed(&mut fx, &source);
    let receipt = take_receipt(&mut fx, &lease, &source).unwrap();
    assert_committed(receipt, &source, "Read deferred.", &fx.file_requests[1]);
    assert_ne!(source, older_source);
}

#[test]
fn cancelling_before_output_authoring_keeps_older_debt_and_creates_no_lease() {
    let mut fx = OutputFixture::new();
    let older = fx.input.run.author(event("Read older.")).unwrap();
    fx.begin(original(vec![text("Read cancelled.")]));
    fx.input.run.context.cancel_output();
    let cancellation = fx.input.run.context.take_cancelled_output().unwrap();
    assert!(cancellation.source.is_none());
    assert_eq!(fx.input.run.context.flow.ledger().source_lease_count(), 0);
    let facts: Vec<_> = fx.input.run.context.flow.ledger().facts().collect();
    assert_eq!(facts.len(), 1);
    assert_eq!(facts[0].source.sequence.0, older.0);
    assert_eq!(fx.input.records(Kind::MessageAssistant).len(), 1);
}

#[test]
fn cancelling_after_output_authoring_releases_only_consumer_interest() {
    let mut fx = OutputFixture::new();
    fx.begin(original(vec![text("Read cancellation.")]));
    let request = fx.request();
    let s::Ste100Target::Source(source) = &request.target else {
        panic!("selected source expected");
    };
    let source = source.clone();
    assert_eq!(fx.input.run.context.flow.ledger().source_lease_count(), 1);
    fx.input.run.context.cancel_output();
    let cancellation = fx.input.run.context.take_cancelled_output().unwrap();
    assert!(cancellation.source.is_some());
    assert_eq!(fx.input.run.context.flow.ledger().source_lease_count(), 0);
    assert!(
        fx.input
            .run
            .context
            .flow
            .ledger()
            .facts()
            .any(|fact| fact.source == &source)
    );
    fx.input.reply(&reply(&request, false));
    await_file(&mut fx, 1);
    answer_file(&mut fx, 0);
    await_disposed(&mut fx, &source);
    assert_eq!(fx.file_requests.len(), 1);
    assert_eq!(fx.input.run.context.flow.ledger().source_lease_count(), 0);
}

#[test]
fn cancelling_a_pending_author_cannot_create_a_lease_when_the_same_ticket_resumes() {
    let mut fx = OutputFixture::new();
    fx.input.fill_trace();
    fx.input
        .run
        .context
        .arm(
            PumpDeadline::Label,
            AbsoluteBound::after(104, 1).unwrap(),
            &mut fx.input.run.clock,
        )
        .unwrap();
    fx.begin(original(vec![text("Read pending cancellation.")]));
    assert!(matches!(
        fx.advance(),
        Err(RunFailure::Wait {
            key: PumpDeadline::Label,
            ..
        })
    ));
    let ticket = fx.input.run.context.pending_author().unwrap().ticket;
    fx.input.run.context.cancel_output();
    assert_eq!(fx.input.run.context.flow.ledger().source_lease_count(), 0);
    assert!(fx.input.run.context.take_cancelled_output().is_none());
    assert_eq!(
        fx.input.run.context.pending_author().unwrap().ticket,
        ticket
    );
    fx.input
        .run
        .context
        .complete_wait(PumpDeadline::Label)
        .unwrap();
    fx.input.run.context.complete_wait(PumpDeadline::Decision);
    fx.input.drain_trace();
    for _ in 0..64 {
        if fx.advance().unwrap() == OutputProgress::Cancelled {
            break;
        }
    }
    assert!(
        fx.input
            .run
            .context
            .take_cancelled_output()
            .unwrap()
            .source
            .is_some()
    );
    assert_eq!(fx.input.run.context.flow.ledger().source_lease_count(), 0);
    assert_eq!(fx.input.run.context.flow.ledger().facts().count(), 1);
    assert_eq!(fx.input.records(Kind::MessageAssistant).len(), 1);
}

#[test]
fn a_generation_lease_coexists_with_a_separate_filing_transaction() {
    let mut fx = OutputFixture::new();
    fx.begin(original(vec![text("Read generation.")]));
    fx.complete(None, false);
    let mut checked = take_generation(&mut fx);
    let generation_source = source(&checked, 0);
    let lease = checked.take_receipt_lease().unwrap();
    fx.input
        .run
        .context
        .begin_filing(
            event("Read separate."),
            PumpDeadline::Decision,
            &mut fx.input.run.clock,
        )
        .unwrap();
    let mut answered = 0;
    let mut complete = false;
    for _ in 0..256 {
        let progress = fx
            .input
            .run
            .context
            .advance_filing(&mut PumpPorts {
                lifecycle: LifecyclePorts::default(),
                gate: None,
                decode: None,
                label: None,
                io: &mut fx.input.run.clock,
            })
            .unwrap();
        if let Some(request) = fx.receive_check() {
            fx.input.reply(&reply(&request, false));
        }
        fx.member(false);
        if fx.file_requests.len() > answered {
            answer_file(&mut fx, answered);
            answered += 1;
        }
        if progress == FilingProgress::Complete {
            complete = true;
            break;
        }
    }
    assert!(complete, "the separate receipt subscription did not settle");
    let separate = fx
        .input
        .run
        .context
        .take_filing(&mut fx.input.run.clock)
        .unwrap()
        .unwrap();
    let dispositions: Vec<_> = separate.receipts().dispositions().collect();
    assert_eq!(dispositions.len(), 1);
    assert_ne!(dispositions[0].source(), &generation_source);
    assert_eq!(fx.file_requests.len(), 2);
    let receipt = take_receipt(&mut fx, &lease, &generation_source).unwrap();
    assert_committed(
        receipt,
        &generation_source,
        "Read generation.",
        &fx.file_requests[0],
    );
    assert!(fx.input.run.context.failure().is_none());
}

#[test]
fn dropping_a_checked_generation_abandons_receipt_interest_but_not_filing() {
    let mut fx = OutputFixture::new();
    fx.begin(original(vec![text("Read abandoned.")]));
    fx.complete(None, false);
    let checked = take_generation(&mut fx);
    let source = source(&checked, 0);
    assert!(checked.receipt_lease().is_some());
    assert_eq!(fx.input.run.context.flow.ledger().source_lease_count(), 1);
    drop(checked);
    assert_eq!(fx.input.run.context.flow.ledger().source_lease_count(), 0);
    assert!(
        fx.input
            .run
            .context
            .flow
            .ledger()
            .facts()
            .any(|fact| fact.source == &source)
    );
    await_file(&mut fx, 1);
    answer_file(&mut fx, 0);
    await_disposed(&mut fx, &source);
    assert_eq!(fx.file_requests.len(), 1);
    assert_eq!(fx.input.run.context.flow.ledger().source_lease_count(), 0);
    assert!(fx.input.run.context.failure().is_none());
}

#[test]
fn an_associated_external_request_uses_its_real_source_receipt_without_a_sibling_barrier() {
    // Keep the synthetic fixture's real checked artifact format, but give recall
    // its own verb so Read is an external statement in this exact loaded scheme.
    let seed: s::SeedRequest = s::decode_strict(include_str!("fixtures/seed.json")).unwrap();
    let mut artifact: serde_json::Value = serde_json::from_str(&seed.ste100_scheme).unwrap();
    let mut recall = artifact["verbs"][0].clone();
    recall["key"] = serde_json::json!("recall");
    recall["forms"] = serde_json::json!(["Recall"]);
    artifact["verbs"].as_array_mut().unwrap().push(recall);
    artifact["recall_keys"] = serde_json::json!(["recall"]);
    let exact = serde_json::to_string(&artifact).unwrap();
    let digest = s::Digest::new(format!("{:x}", Sha256::digest(exact.as_bytes()))).unwrap();
    let declaration = s::SchemeDeclaration {
        scheme_id: format!("ste100:{digest}"),
        artifact_sha256: digest,
        verbs: vec!["read".into(), "recall".into()],
    };
    let scheme = RequestScheme::from_exact_scheme(&exact, &declaration).unwrap();
    let mut fx = OutputFixture::with_scheme(exact, declaration);
    let arguments = "printf '%s\\n' 'opaque { command }'\n";
    fx.begin(original(vec![
        text("Read request."),
        ContentBlock::ToolCall(ToolCall {
            name: "sh".into(),
            arguments: arguments.into(),
        }),
        text("Read sibling."),
    ]));
    fx.complete(None, false);
    let checked = take_generation(&mut fx);
    let source = source(&checked, 0);
    let sibling = s::SourceBlockId {
        block: 2.into(),
        ..source.clone()
    };
    let requests = OriginalRequests::from_checked(checked, &scheme).unwrap();
    assert_eq!(requests.requests().len(), 1);
    assert!(requests.unresolved().is_empty());
    assert!(requests.refusals().is_empty());
    let candidate = &requests.requests()[0];
    assert_eq!(
        candidate.opening().call_id,
        s::CallId {
            run: source.run.clone(),
            sequence: source.sequence,
            call: 0.into(),
        }
    );
    assert_eq!(candidate.opening().call_block, 1.into());
    assert_eq!(candidate.dependency().block_id, source);
    assert_eq!(candidate.dependency().binding, 0.into());
    assert_eq!(
        candidate.opening().request,
        s::PairRequest::External {
            name: "sh".into(),
            arguments: arguments.into(),
        }
    );
    let dependency = candidate.dependency().block_id.clone();
    let mut checked = requests.into_generation();
    let lease = checked.take_receipt_lease().unwrap();
    await_file(&mut fx, 1);
    answer_file(&mut fx, 0);
    await_disposed(&mut fx, &dependency);
    assert!(take_receipt(&mut fx, &lease, &sibling).is_none());
    let receipt = take_receipt(&mut fx, &lease, &dependency).unwrap();
    assert_committed(receipt, &dependency, "Read request.", &fx.file_requests[0]);
    assert!(
        fx.input
            .run
            .context
            .flow
            .ledger()
            .facts()
            .any(|fact| fact.source == &sibling)
    );
    assert_eq!(fx.requests.len(), 2);
    assert_eq!(fx.input.records(Kind::MessageAssistant).len(), 1);
    assert!(fx.input.run.context.failure().is_none());
}
