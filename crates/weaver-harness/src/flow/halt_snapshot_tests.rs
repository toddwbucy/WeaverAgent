use super::*;
use crate::flow::context::tests::with_frozen_source;
use crate::flow::coordinator::{FlowRecording, tests::Fixture};
use crate::flow::report::SecondaryFailure;
use crate::flow::report::{CaptureState, PairState};
use weaver_types::state_flow::{self as s, CallId};

fn latch(fx: &mut Fixture, reason: FlowHaltReason, point: HaltPoint, detail: &str) {
    let observed_ns = Some(fx.time().into());
    fx.flow.latch(
        SecondaryFailure {
            reason,
            point,
            detail: detail.into(),
            observed_ns,
        },
        None,
        &mut FlowRecording {
            barrier: &mut fx.barrier,
            inbox: &mut fx.inbox,
            budget: &mut fx.budget,
            author: &fx.author,
        },
    );
}

fn take_snapshot(fx: &Fixture) -> FlowHaltAccount {
    snapshot(&fx.flow, &fx.barrier, &fx.inbox, &fx.budget, external()).unwrap()
}

// Explicit absence belongs only to these fixtures, which have no input, tool or
// capture owner. Production callers must supply their actual owner's facts.
fn external() -> ExternalFacts {
    ExternalFacts {
        outstanding_pairs: Vec::new(),
        capture: None,
        wait: None,
        input_id: None,
    }
}

#[test]
fn snapshot_uses_the_real_latched_source_and_cached_sample_without_reduction() {
    with_frozen_source(|flow, barrier, inbox, budget| {
        let first = flow.halt().unwrap().first();
        let before = budget.gauges();
        let sources: Vec<_> = flow
            .ledger()
            .facts()
            .map(|fact| fact.source.clone())
            .collect();
        assert!(!sources.is_empty());
        let account = snapshot(flow, barrier, inbox, budget, external()).unwrap();
        assert_eq!(account.reason, first.reason);
        assert_eq!(account.point, first.point);
        assert_eq!(account.detail, first.detail);
        assert_eq!(account.observed_ns, first.observed_ns);
        assert_eq!(&account.flow_sample, flow.final_sample().unwrap().unwrap());
        assert_eq!(
            account
                .outstanding_blocks
                .iter()
                .map(|block| block.block_id.clone())
                .collect::<Vec<_>>(),
            sources
        );
        assert!(account.outstanding_blocks.iter().all(|block| block.state
            == BlockState::AwaitingClassification
            && block.commit == CommitEvidence::NotRequested
            && block.bytes_sent.0 == 0
            && block.request_id.is_none()
            && block.frame_bytes.is_none()));
        // A source admitted before an unrelated later trace failure is not the
        // failed author. Its identity stays in the outstanding ledger instead.
        assert!(account.source_event.is_none());
        assert!(account.source_id.is_none());
        assert!(account.pending_author.is_none());
        assert!(account.secondary.is_empty());
        assert_eq!(budget.gauges(), before);
        assert_eq!(
            flow.ledger()
                .facts()
                .map(|fact| fact.source.clone())
                .collect::<Vec<_>>(),
            sources
        );
        let again = snapshot(flow, barrier, inbox, budget, external()).unwrap();
        assert_eq!(
            account.to_raw().unwrap().get(),
            again.to_raw().unwrap().get()
        );
    });
}

#[test]
fn explicit_external_obligations_are_preserved_without_adopting_defaults() {
    with_frozen_source(|flow, barrier, inbox, budget| {
        let opening_source = flow.ledger().facts().next().unwrap().source.clone();
        let pair = OutstandingPair {
            call_id: CallId {
                run: opening_source.run.clone(),
                sequence: opening_source.sequence,
                call: 0.into(),
            },
            state: PairState::Opening,
            execution: None,
            opening_source,
            opening_filed: false,
            terminal_source: None,
            terminal_filed: false,
        };
        let capture = CaptureAtHalt {
            capture_id: 9.into(),
            state: CaptureState::AwaitPublished,
            commit_sent: true,
            published: None,
        };
        let wait = WaitAtHalt {
            stage: trace::BlockedStage::Control,
            began_ns: 100.into(),
            deadline_ns: 10000.into(),
        };
        let account = snapshot(
            flow,
            barrier,
            inbox,
            budget,
            ExternalFacts {
                outstanding_pairs: vec![pair.clone()],
                capture: Some(capture.clone()),
                wait: Some(wait.clone()),
                input_id: Some(17.into()),
            },
        )
        .unwrap();
        assert_eq!(account.outstanding_pairs, vec![pair]);
        assert_eq!(account.capture, Some(capture));
        assert_eq!(account.wait, Some(wait));
        assert_eq!(account.input_id, Some(17.into()));
        assert_eq!(account.reason, FlowHaltReason::TraceFailed);
        assert!(account.request.is_none());
    });
}

#[test]
fn failed_snapshot_returns_every_external_obligation_to_its_owner() {
    with_frozen_source(|flow, barrier, inbox, budget| {
        let opening_source = flow.ledger().facts().next().unwrap().source.clone();
        let pair = OutstandingPair {
            call_id: CallId {
                run: opening_source.run.clone(),
                sequence: opening_source.sequence,
                call: 1.into(),
            },
            state: PairState::Ending,
            execution: None,
            opening_source,
            opening_filed: true,
            terminal_source: None,
            terminal_filed: true,
        };
        let capture = CaptureAtHalt {
            capture_id: 7.into(),
            state: CaptureState::Beginning,
            commit_sent: false,
            published: None,
        };
        let failed = snapshot(
            flow,
            barrier,
            inbox,
            budget,
            ExternalFacts {
                outstanding_pairs: vec![pair.clone()],
                capture: Some(capture.clone()),
                wait: None,
                input_id: Some(12.into()),
            },
        )
        .unwrap_err();
        assert_eq!(
            failed.error,
            ReportError::Invalid("filed pair terminal has no source")
        );
        assert_eq!(failed.external.outstanding_pairs, vec![pair]);
        assert_eq!(failed.external.capture, Some(capture));
        assert_eq!(failed.external.input_id, Some(12.into()));
        assert_eq!(
            flow.halt().unwrap().first().detail,
            "fixture failure after admission"
        );
    });
}

#[test]
fn a_copy_snapshot_keeps_sparse_ordinals_and_refuses_cross_event_blocks() {
    use std::sync::Arc;
    use weaver_trace::serving::{PathPart, SelectedBlock, SourceBlock, StatementCopy};
    let make = || {
        AdmittedCopy::Statement(StatementCopy {
            source_envelope: Arc::from(concat!(
                "{\"session\":\"session\",\"run\":\"run\",",
                "\"sequence\":\"8\",\"kind\":\"message.assistant\",",
                "\"channel\":\"output\",\"origin\":\"model\",",
                "\"subsystem\":\"harness\",\"wall_ms\":1,",
                "\"monotonic_ns\":\"7\"}"
            )),
            partition_rule_id: "fixture selected ordinals".into(),
            blocks: [0, 4, 9]
                .into_iter()
                .map(|block| SelectedBlock {
                    source: SourceBlock {
                        run: "run".into(),
                        sequence: weaver_trace::Sequence(8),
                        block,
                    },
                    source_path: vec![PathPart::Index(block)],
                    raw: Arc::from("{}"),
                })
                .collect(),
            context: None,
        })
    };
    let copy = make();
    let identity = copy_identity(&copy).unwrap();
    assert_eq!(
        identity
            .sources
            .iter()
            .map(|id| id.block.0)
            .collect::<Vec<_>>(),
        [0, 4, 9]
    );
    assert_eq!(identity.source_event.sequence.0, 8);
    let AdmittedCopy::Statement(mut changed) = make() else {
        unreachable!()
    };
    changed.blocks[1].source.sequence = weaver_trace::Sequence(9);
    assert!(matches!(
        copy_identity(&AdmittedCopy::Statement(changed)),
        Err(ReportError::Invalid(_))
    ));
    let AdmittedCopy::Statement(mut changed) = make() else {
        unreachable!()
    };
    changed.blocks.swap(1, 2);
    assert!(matches!(
        copy_identity(&AdmittedCopy::Statement(changed)),
        Err(ReportError::Invalid(_))
    ));
    let AdmittedCopy::Statement(mut empty) = make() else {
        unreachable!()
    };
    empty.blocks.clear();
    let zero = copy_identity(&AdmittedCopy::Statement(empty)).unwrap();
    assert!(zero.sources.is_empty());
    assert_eq!(zero.source_event, identity.source_event);
}

#[test]
fn live_owners_cannot_be_snapshotted_before_the_sole_latch() {
    let fx = Fixture::new(4);
    let failure = snapshot(&fx.flow, &fx.barrier, &fx.inbox, &fx.budget, external()).unwrap_err();
    assert_eq!(failure.error, ReportError::SnapshotMissing);
    assert!(!fx.budget.is_frozen());
    assert!(fx.flow.halt().is_none());
}

#[test]
fn queued_and_waiting_whole_copies_remain_distinct_from_ledger_blocks() {
    let mut fx = Fixture::new(3);
    let installed = fx.admit(&["Read first.", "Read second."]);
    fx.advance().unwrap();
    let queued = fx.admit(&["Read third."]);
    let waiting = fx.admit(&["Read fourth.", "Read fifth."]);
    assert_eq!(fx.inbox.queued().count(), 1);
    assert!(fx.budget.waiting_copy().is_some());
    let expired = fx.time();
    latch(
        &mut fx,
        FlowHaltReason::QueueLimit,
        HaltPoint::Barrier,
        "decision expired behind whole copy",
    );
    let mut outside = external();
    outside.wait = Some(WaitAtHalt {
        stage: trace::BlockedStage::Decision,
        began_ns: (expired - 1).into(),
        deadline_ns: expired.into(),
    });
    let account = snapshot(&fx.flow, &fx.barrier, &fx.inbox, &fx.budget, outside).unwrap();
    assert_eq!(
        account
            .outstanding_blocks
            .iter()
            .map(|block| block.block_id.clone())
            .collect::<Vec<_>>(),
        installed
    );
    assert_eq!(account.queued_copies.len(), 1);
    assert_eq!(
        account.queued_copies[0]
            .sources
            .iter()
            .map(|id| (id.sequence.0, id.block.0))
            .collect::<Vec<_>>(),
        queued
            .iter()
            .map(|id| (id.sequence.0, id.block.0))
            .collect::<Vec<_>>()
    );
    assert_eq!(
        account
            .unheld_copy
            .as_ref()
            .unwrap()
            .sources
            .iter()
            .map(|id| (id.sequence.0, id.block.0))
            .collect::<Vec<_>>(),
        waiting
            .iter()
            .map(|id| (id.sequence.0, id.block.0))
            .collect::<Vec<_>>()
    );
    assert_eq!(account.unheld_copy, account.flow_sample.waiting_copy);
    assert_eq!(account.flow_sample.pending_count.0, 3);
    assert_eq!(fx.inbox.queued().count(), 1);
    assert!(fx.budget.waiting_copy().is_some());
}

#[test]
fn fully_sent_request_without_answer_stays_unknown_and_correlated_after_eof() {
    let mut fx = Fixture::new(4);
    let sources = fx.admit(&["Read first.", "Read second."]);
    fx.until_file();
    let progress = fx.flow.state().outstanding()[0].clone();
    assert_eq!(progress.bytes_sent, progress.frame_bytes);
    fx.state_peer.shutdown(std::net::Shutdown::Write).unwrap();
    fx.transport().unwrap_err();
    latch(
        &mut fx,
        FlowHaltReason::CarryFailed,
        HaltPoint::StateReply,
        "state EOF at reply",
    );
    let account = take_snapshot(&fx);
    assert_eq!(account.source_id, Some(sources[0].clone()));
    assert_eq!(account.request, Some(state_request(&progress)));
    assert!(account.received_request.is_none());
    let block = &account.outstanding_blocks[0];
    assert_eq!(block.state, BlockState::AwaitingReceipt);
    assert_eq!(block.commit, CommitEvidence::Unknown);
    assert_eq!(block.bytes_sent.0, progress.bytes_sent as u64);
    assert_eq!(block.frame_bytes.unwrap().0, progress.frame_bytes as u64);
    assert_eq!(
        account.outstanding_blocks[1].commit,
        CommitEvidence::NotRequested
    );
    assert_eq!(account.outstanding_blocks[1].block_id, sources[1]);
}

#[test]
fn matching_filed_result_keeps_committed_evidence_before_its_trace_release() {
    let mut fx = Fixture::new(4);
    let sources = fx.admit(&["Read first."]);
    fx.until_file();
    fx.answer_file(0, false);
    fx.transport().unwrap();
    assert!(fx.flow.state().held_measurement().is_some());
    assert_eq!(
        fx.flow.state().outstanding()[0].commit,
        CommitEvidence::Committed
    );
    latch(
        &mut fx,
        FlowHaltReason::TraceFailed,
        HaltPoint::Author,
        "recording stopped before measurement",
    );
    let account = take_snapshot(&fx);
    assert_eq!(account.outstanding_blocks[0].block_id, sources[0]);
    assert_eq!(
        account.outstanding_blocks[0].commit,
        CommitEvidence::Committed
    );
    assert_eq!(
        account.outstanding_blocks[0].state,
        BlockState::AwaitingReceipt
    );
    assert!(fx.flow.state().held_measurement().is_some());
    assert!(fx.flow.ledger().facts().next().is_some());
}

#[test]
fn wrong_reply_identity_reports_both_ids_and_never_invents_a_commit() {
    let mut fx = Fixture::new(4);
    let sources = fx.admit(&["Read first."]);
    fx.until_file();
    let expected = fx.requests[0].common().request_id;
    fx.answer_file(0, true);
    fx.transport().unwrap_err();
    latch(
        &mut fx,
        FlowHaltReason::ProtocolFailed,
        HaltPoint::StateReply,
        "wrong reply request ID",
    );
    let account = take_snapshot(&fx);
    assert_eq!(account.request.unwrap().id, expected);
    assert_eq!(account.received_request.unwrap().id.0, expected.0 + 1);
    assert_eq!(account.source_id, Some(sources[0].clone()));
    assert_eq!(
        account.outstanding_blocks[0].commit,
        CommitEvidence::Unknown
    );
    assert!(fx.flow.state().held_measurement().is_none());
}

#[test]
fn matching_refusal_keeps_refused_commit_and_exact_request_while_measurement_is_held() {
    use std::io::Write;
    let mut fx = Fixture::new(4);
    let sources = fx.admit(&["Read first."]);
    fx.until_file();
    let now = fx.time();
    let request = &fx.requests[0];
    let answer = s::StateFrame::Refused {
        common: request.common().clone(),
        code: s::StateRefusalCode::WriteFailed,
        detail: "fixture transaction refusal".into(),
        block_id: Some(sources[0].clone()),
        observation: crate::flow::coordinator::tests::operation(request, 2, now + 1),
    };
    fx.state_peer
        .write_all(&s::encode_state_line(&answer, s::FrameDirection::Response).unwrap())
        .unwrap();
    fx.elapse(3);
    fx.transport().unwrap();
    assert_eq!(
        fx.flow.state().outstanding()[0].commit,
        CommitEvidence::Refused
    );
    latch(
        &mut fx,
        FlowHaltReason::TraceFailed,
        HaltPoint::Author,
        "trace stopped with a refusal held",
    );
    let account = take_snapshot(&fx);
    assert_eq!(
        account.outstanding_blocks[0].commit,
        CommitEvidence::Refused
    );
    assert_eq!(
        account.outstanding_blocks[0].request_id,
        Some(answer.common().request_id)
    );
    assert_eq!(account.outstanding_blocks[0].block_id, sources[0]);
    assert!(fx.flow.state().held_measurement().is_some());
}

#[test]
fn classifier_mismatch_keeps_expected_source_and_received_id_without_using_bad_target() {
    use crate::flow::classifier::scripted::ScriptStep;
    let mut fx = Fixture::new(4);
    let sources = fx.admit(&["Read first."]);
    let request = fx.until_classifier_request();
    let reply = s::Ste100Reply {
        request_id: 0.into(),
        target: s::Ste100Target::Input {
            input_id: 999.into(),
            block: 0.into(),
        },
        scheme_id: request.scheme_id.clone(),
        result: s::Ste100Result::NotSte100 {
            code: s::Ste100DefectCode::NonSte100,
            detail: "mismatched fixture".into(),
            spans: vec![],
        },
    };
    fx.classifier.push(ScriptStep::Reply(reply));
    fx.classifier.advance().unwrap();
    fx.transport().unwrap_err();
    latch(
        &mut fx,
        FlowHaltReason::ClassifierFailed,
        HaltPoint::Classifier,
        "classifier returned mismatched identity",
    );
    let account = take_snapshot(&fx);
    assert_eq!(account.source_id, Some(sources[0].clone()));
    assert_eq!(account.request.unwrap().id, request.request_id);
    assert_eq!(account.received_request.unwrap().id.0, 0);
    assert!(account.input_id.is_none());
    // A separate frame owner can be waiting behind this source's check. Its
    // explicit identity is independent of the rejected reply's bogus target.
    let mut independent_input = external();
    independent_input.input_id = Some(17.into());
    let with_input = snapshot(
        &fx.flow,
        &fx.barrier,
        &fx.inbox,
        &fx.budget,
        independent_input,
    )
    .unwrap();
    assert_eq!(with_input.input_id, Some(17.into()));
    assert_eq!(with_input.source_id, Some(sources[0].clone()));
    assert_eq!(with_input.request.unwrap().id, request.request_id);
    assert_eq!(with_input.received_request.unwrap().id.0, 0);
}

#[test]
fn unadmitted_original_author_is_kept_with_its_ticket_and_no_fabricated_source() {
    use crate::flow::author::{AuthoredEvent, AuthoredSource};
    use crate::flow::recording::{RecordProgress, RecordPurpose};
    use weaver_trace::serving::{Kind, Payload};
    use weaver_trace::{Subsystem, TurnRef};
    let mut fx = Fixture::new(4);
    let event = AuthoredEvent {
        kind: Kind::MessageAssistant, subsystem: Subsystem::Harness,
        turn: Some(TurnRef("turn".into())), source: AuthoredSource::Model, causal_parent: None,
        payload: Some(Payload::Message(serde_json::value::RawValue::from_string(
            "{ \"role\": \"assistant\", \"content\": [{\"type\":\"text\", \"text\":\"Read original.\"}] }".into()).unwrap())),
    };
    let original = match &event.payload {
        Some(Payload::Message(raw)) => raw.get().to_owned(),
        _ => unreachable!(),
    };
    assert!(matches!(
        fx.barrier.submit_authored_with_clock(
            event,
            RecordPurpose::Content,
            &fx.author,
            1,
            &mut fx.inbox,
            &mut fx.budget,
            &mut || Err(std::io::Error::other("fixture clock unavailable"))
        ),
        RecordProgress::Failed
    ));
    let ticket = fx.barrier.pending_author().unwrap().ticket;
    latch(
        &mut fx,
        FlowHaltReason::TraceFailed,
        HaltPoint::Author,
        "author clock unavailable",
    );
    let account = take_snapshot(&fx);
    assert!(account.source_event.is_none());
    assert!(account.source_id.is_none());
    let pending = account.pending_author.as_ref().unwrap();
    assert_eq!(pending.ticket.0, ticket);
    let Some(Payload::Message(raw)) = &pending.event.payload else {
        panic!("original message");
    };
    assert_eq!(raw.get(), original);
    assert_eq!(fx.barrier.pending_author().unwrap().ticket, ticket);
    assert!(account.outstanding_blocks.is_empty());
}

fn message(raw: &str) -> crate::flow::author::AuthoredEvent {
    use crate::flow::author::{AuthoredEvent, AuthoredSource};
    AuthoredEvent {
        kind: weaver_trace::serving::Kind::MessageAssistant,
        subsystem: weaver_trace::Subsystem::Harness,
        turn: Some(weaver_trace::TurnRef("turn".into())),
        source: AuthoredSource::Model,
        causal_parent: None,
        payload: Some(weaver_trace::serving::Payload::Message(
            serde_json::value::RawValue::from_string(raw.into()).unwrap(),
        )),
    }
}

#[test]
fn failed_copy_intake_keeps_the_whole_admitted_copy_and_its_real_sequence() {
    use crate::flow::recording::{RecordProgress, RecordPurpose};
    let mut fx = Fixture::new(4);
    let before = fx.admit(&["Read retained."]);
    fx.advance().unwrap();
    let now = fx.time();
    let mut reads = 0;
    let result = fx.barrier.submit_authored_with_clock(
        message("{\"role\":\"assistant\",\"content\":[{\"type\":\"text\",\"text\":\"Read first.\"},{\"type\":\"text\",\"text\":\"Read second.\"}]}"),
        RecordPurpose::Content,
        &fx.author,
        1,
        &mut fx.inbox,
        &mut fx.budget,
        &mut || {
            reads += 1;
            if reads == 1 { Ok(now) } else { Err(std::io::Error::other("intake clock unavailable")) }
        },
    );
    assert!(matches!(result, RecordProgress::Failed));
    let sequence = fx.barrier.admitted_before_failure().unwrap();
    assert!(fx.inbox.failure().is_some());
    assert!(fx.barrier.pending_author().is_none());
    latch(
        &mut fx,
        FlowHaltReason::TapFailed,
        HaltPoint::CopyIntake,
        "copy intake clock unavailable",
    );
    let account = take_snapshot(&fx);
    let copy = account.unheld_copy.as_ref().unwrap();
    assert_eq!(copy.source_event.sequence.0, sequence.0);
    assert_eq!(
        copy.sources.iter().map(|id| id.block.0).collect::<Vec<_>>(),
        [0, 1]
    );
    assert_eq!(account.source_event, Some(copy.source_event.clone()));
    assert!(
        account.source_id.is_none(),
        "a whole-copy failure does not select one block"
    );
    assert_eq!(account.outstanding_blocks[0].block_id, before[0]);
    assert!(account.pending_author.is_none());
    assert!(fx.inbox.failure().is_some());
}

#[test]
fn a_real_tap_failure_names_only_the_admitted_event_not_an_invented_block() {
    use crate::flow::recording::{RecordProgress, RecordPurpose};
    let mut fx = Fixture::new(4);
    let now = fx.time();
    let result = fx.barrier.submit_authored_with_clock(
        message("{\"role\":\"assistant\",\"content\":17}"),
        RecordPurpose::Content,
        &fx.author,
        1,
        &mut fx.inbox,
        &mut fx.budget,
        &mut || Ok(now),
    );
    assert!(matches!(result, RecordProgress::Failed));
    let Some(RecordFailure::Tap(tap)) = fx.barrier.failure() else {
        panic!("actual tap failure");
    };
    let source = tap.source.clone();
    latch(
        &mut fx,
        FlowHaltReason::TapFailed,
        HaltPoint::Tap,
        "selected content projection failed",
    );
    let account = take_snapshot(&fx);
    assert_eq!(
        account.source_event.unwrap(),
        trace::CopySourceEvent {
            run: source.run,
            sequence: source.sequence.0.into()
        }
    );
    assert!(account.source_id.is_none());
    assert!(account.unheld_copy.is_none());
    assert!(account.pending_author.is_none());
    assert!(account.outstanding_blocks.is_empty());
}

#[test]
fn preopen_halt_has_no_synthetic_measurement_baseline() {
    use crate::flow::channel::StateChannel;
    use crate::flow::classifier::scripted::ScriptedPort;
    use std::os::unix::net::UnixStream;
    let mut fx = Fixture::new(4);
    let (socket, _peer) = UnixStream::pair().unwrap();
    let digest = s::Digest::new("a".repeat(64)).unwrap();
    let mut preflight = FlowCoordinator::<ScriptedPort>::preflight(
        StateChannel::new(socket, "session".into(), "run".into()).unwrap(),
        s::SchemeDeclaration {
            scheme_id: format!("ste100:{}", digest.as_str()),
            artifact_sha256: digest,
            verbs: vec!["read".into()],
        },
        s::MeasuredInterval {
            elapsed_ns: 1000.into(),
            measurement_ref: "test-only preopen cadence".into(),
        },
        2,
    )
    .unwrap();
    let now = fx.time();
    preflight.latch(
        SecondaryFailure {
            reason: FlowHaltReason::ProtocolFailed,
            point: HaltPoint::Barrier,
            observed_ns: Some(now.into()),
            detail: "before open".into(),
        },
        None,
        &mut FlowRecording {
            barrier: &mut fx.barrier,
            inbox: &mut fx.inbox,
            budget: &mut fx.budget,
            author: &fx.author,
        },
    );
    let failed = snapshot(&preflight, &fx.barrier, &fx.inbox, &fx.budget, external()).unwrap_err();
    assert_eq!(failed.error, ReportError::SnapshotMissing);
    assert!(preflight.final_sample().unwrap().is_none());
    assert_eq!(preflight.halt().unwrap().first().detail, "before open");
}
