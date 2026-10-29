use super::*;
use serde_json::{Value, json};
use weaver_trace::serving::{Kind, Payload, payloads as trace};
use weaver_trace::{Sequence, Subsystem, TurnRef};
use weaver_types::{Opener, state_flow::Digest};

fn source(sequence: u64, block: u64) -> SourceBlockId {
    SourceBlockId {
        run: "run".into(),
        sequence: sequence.into(),
        block: block.into(),
    }
}

fn copy(sequence: u64) -> CopyAtHalt {
    CopyAtHalt {
        source_event: CopySourceEvent {
            run: "run".into(),
            sequence: sequence.into(),
        },
        partition_rule_id: "statement-rule".into(),
        content_type: CopyContentType::Statement,
        sources: vec![trace::SourceBlockId {
            run: "run".into(),
            sequence: sequence.into(),
            block: 0.into(),
        }],
    }
}

fn sample() -> FlowObservation {
    FlowObservation {
        phase: FlowPhase::Sampled,
        source_id: None,
        input_id: None,
        request_id: None,
        pending_count: 2.into(),
        oldest_age_ns: 20.into(),
        elapsed_ns: None,
        blocked_stage: None,
        sample_start_ns: 100.into(),
        sampled_ns: 120.into(),
        pending_bytes: 84.into(),
        filed_count: 3.into(),
        filed_bytes: 140.into(),
        rejected_count: 1.into(),
        input_pending: 0.into(),
        input_started: 1.into(),
        input_completed: 1.into(),
        retained_blocks: 2.into(),
        retained_bytes: 420.into(),
        pending_turns: 1.into(),
        backlog_waiting: true,
        lag_alarm: true,
        waiting_copy: Some(copy(4)),
        failed_corrections: None,
        retry_cap: None,
    }
}

fn account() -> FlowHaltAccount {
    FlowHaltAccount {
        reason: FlowHaltReason::CarryFailed,
        point: HaltPoint::StateReply,
        detail: "state EOF after complete request".into(),
        observed_ns: Some(120.into()),
        source_event: Some(CopySourceEvent { run: "run".into(), sequence: 2.into() }),
        source_id: Some(source(2, 0)),
        input_id: None,
        request: Some(RequestAtHalt { domain: RequestDomain::State, id: 7.into() }),
        received_request: None,
        wait: Some(WaitAtHalt {
            stage: BlockedStage::Decision, began_ns: 100.into(), deadline_ns: 200.into(),
        }),
        outstanding_blocks: vec![BlockAtHalt {
            block_id: source(2, 0),
            content_type: CopyContentType::Statement,
            state: BlockState::AwaitingReceipt,
            request_id: Some(7.into()),
            bytes_sent: 99.into(),
            frame_bytes: Some(99.into()),
            commit: CommitEvidence::Unknown,
        }],
        queued_copies: vec![copy(3)],
        unheld_copy: Some(copy(4)),
        pending_author: Some(AuthorAtHalt {
            ticket: 17.into(),
            event: AuthoredEvent {
                kind: Kind::MessageAssistant,
                subsystem: Subsystem::Harness,
                turn: Some(TurnRef("turn".into())),
                source: AuthoredSource::Model,
                causal_parent: Some(Sequence(2)),
                payload: Some(Payload::Message(RawValue::from_string(
                    r#"{ "role": "assistant", "content": [{"type":"text","text":"Read file."}] }"#.into(),
                ).unwrap())),
            },
        }),
        outstanding_pairs: vec![OutstandingPair {
            call_id: CallId { run: "run".into(), sequence: 1.into(), call: 0.into() },
            state: PairState::Executing,
            execution: Some(ExchangeId { opener: Opener::Harness, ordinal: 23 }),
            opening_source: source(5, 0),
            opening_filed: true,
            terminal_source: None,
            terminal_filed: false,
        }],
        capture: None,
        flow_sample: sample(),
        secondary: vec![],
    }
}

fn cause(account: &FlowHaltAccount) -> SecondaryFailure {
    SecondaryFailure {
        reason: account.reason,
        point: account.point,
        observed_ns: account.observed_ns,
        detail: account.detail.clone(),
    }
}

fn decoded(account: &FlowHaltAccount) -> Value {
    serde_json::from_str(account.to_raw().unwrap().get()).unwrap()
}

#[test]
fn complete_account_has_exact_required_fields_nulls_and_original_author_bytes() {
    let account = account();
    let raw = account.to_raw().unwrap();
    assert_eq!(account.account_bytes().unwrap(), raw.get().len());
    let value: Value = serde_json::from_str(raw.get()).unwrap();
    assert_eq!(value.as_object().unwrap().len(), 18);
    assert_eq!(value["reason"], "carry_failed");
    assert_eq!(value["point"], "state_reply");
    assert_eq!(value["observed_ns"], "120");
    for field in ["input_id", "received_request", "capture"] {
        assert!(value.as_object().unwrap().contains_key(field));
        assert!(value[field].is_null());
    }
    assert_eq!(value["request"], json!({"domain":"state","id":"7"}));
    assert_eq!(value["outstanding_blocks"][0]["bytes_sent"], "99");
    assert_eq!(value["outstanding_blocks"][0]["commit"], "unknown");
    assert_eq!(value["queued_copies"][0]["source_event"]["sequence"], "3");
    assert_eq!(value["unheld_copy"]["source_event"]["sequence"], "4");
    assert_eq!(value["pending_author"]["ticket"], "17");
    let event = &value["pending_author"]["event"];
    assert_eq!(event.as_object().unwrap().len(), 6);
    assert_eq!(event["kind"], "message.assistant");
    assert_eq!(event["subsystem"], "harness");
    assert_eq!(event["turn"], "turn");
    assert_eq!(event["source"], "model");
    assert_eq!(event["causal_parent"], "2");
    assert!(event.get("sequence").is_none());
    let Payload::Message(original) = account
        .pending_author
        .as_ref()
        .unwrap()
        .event
        .payload
        .as_ref()
        .unwrap()
    else {
        panic!("message fixture");
    };
    assert!(
        raw.get().contains(original.get()),
        "raw payload spacing survives"
    );
    assert_eq!(
        value["flow_sample"],
        serde_json::to_value(&account.flow_sample).unwrap()
    );
    // ExchangeId is the existing shared carrier, including its ordinal spelling.
    assert_eq!(
        value["outstanding_pairs"][0]["execution"],
        json!({"opener":"harness","ordinal":23})
    );
    assert!(value["outstanding_pairs"][0]["terminal_source"].is_null());
    assert_eq!(value["secondary"], json!([]));
}

#[test]
fn original_author_sources_are_externally_tagged_and_optional_members_stay_null() {
    for (source, expected) in [
        (
            AuthoredSource::BoundaryInput(4242),
            json!({"boundary_input":4242}),
        ),
        (
            AuthoredSource::ManagementInput(crate::flow::plan::ManagementInput::for_test_root()),
            json!({"management_input":{"uid":0}}),
        ),
        (AuthoredSource::Model, json!("model")),
        (AuthoredSource::Tool, json!("tool")),
        (AuthoredSource::Loop, json!("loop")),
        (AuthoredSource::Runtime, json!("runtime")),
    ] {
        let mut account = account();
        let author = &mut account.pending_author.as_mut().unwrap().event;
        author.source = source;
        author.turn = None;
        author.causal_parent = None;
        author.payload = None;
        let value = decoded(&account);
        let event = &value["pending_author"]["event"];
        assert_eq!(event["source"], expected);
        for key in ["turn", "causal_parent", "payload"] {
            assert!(event.as_object().unwrap().contains_key(key));
            assert!(event[key].is_null());
        }
    }
}

#[test]
fn absent_correlation_stays_null_and_a_decoded_bad_reply_id_is_not_invented() {
    let mut account = account();
    account.source_event = None;
    account.source_id = None;
    account.input_id = None;
    account.request = None;
    account.wait = None;
    account.pending_author = None;
    account.received_request = Some(RequestAtHalt {
        domain: RequestDomain::Ste100,
        id: 0.into(),
    });
    let value = decoded(&account);
    for key in [
        "source_event",
        "source_id",
        "input_id",
        "request",
        "wait",
        "pending_author",
    ] {
        assert!(value.as_object().unwrap().contains_key(key));
        assert!(value[key].is_null());
    }
    assert_eq!(
        value["received_request"],
        json!({"domain":"ste100","id":"0"})
    );
    account.source_event = Some(CopySourceEvent {
        run: "other".into(),
        sequence: 2.into(),
    });
    account.source_id = Some(source(2, 0));
    assert_eq!(
        account.to_raw().unwrap_err(),
        ReportError::Invalid("source and event identities disagree")
    );
}

#[test]
fn each_bound_requires_its_original_stage_and_observed_expiry() {
    for (reason, stage) in [
        (FlowHaltReason::GateBound, BlockedStage::Gate),
        (FlowHaltReason::CutBound, BlockedStage::Cut),
        (FlowHaltReason::DecisionBound, BlockedStage::Decision),
        (FlowHaltReason::ControlBound, BlockedStage::Control),
        (FlowHaltReason::QueueLimit, BlockedStage::Decision),
    ] {
        let mut account = account();
        account.reason = reason;
        account.point = HaltPoint::Barrier;
        account.wait = Some(WaitAtHalt {
            stage,
            began_ns: 100.into(),
            deadline_ns: 120.into(),
        });
        account.validate().unwrap();
        account.observed_ns = Some(119.into());
        assert!(account.validate().is_err());
        account.observed_ns = None;
        assert!(account.validate().is_err());
        account.observed_ns = Some(120.into());
        account.wait.as_mut().unwrap().stage = if stage == BlockedStage::Gate {
            BlockedStage::Cut
        } else {
            BlockedStage::Gate
        };
        assert!(account.validate().is_err());
        account.wait = None;
        assert!(account.validate().is_err());
    }
}

#[test]
fn queue_limit_requires_the_same_whole_waiting_copy_without_an_immediate_full_fault() {
    let mut account = account();
    account.reason = FlowHaltReason::QueueLimit;
    account.point = HaltPoint::Barrier;
    account.wait.as_mut().unwrap().deadline_ns = 120.into();
    account.validate().unwrap();
    for defect in 0..3 {
        let mut changed = account.clone();
        match defect {
            0 => changed.unheld_copy = None,
            1 => changed.flow_sample.backlog_waiting = false,
            _ => changed
                .unheld_copy
                .as_mut()
                .unwrap()
                .partition_rule_id
                .push_str("-different"),
        }
        assert!(changed.to_raw().is_err());
    }
}

#[test]
fn commit_evidence_preserves_partial_unknown_committed_and_refused_distinctions() {
    for (commit, bytes, frame, word) in [
        (CommitEvidence::NotRequested, 0, None, "not_requested"),
        (CommitEvidence::NotCommitted, 5, Some(99), "not_committed"),
        (CommitEvidence::Unknown, 99, Some(99), "unknown"),
        (CommitEvidence::Committed, 99, Some(99), "committed"),
        (CommitEvidence::Refused, 99, Some(99), "refused"),
    ] {
        let mut account = account();
        let block = &mut account.outstanding_blocks[0];
        block.commit = commit;
        block.bytes_sent = bytes.into();
        block.frame_bytes = frame.map(DecimalU64);
        let value = decoded(&account);
        assert_eq!(value["outstanding_blocks"][0]["commit"], word);
        assert_eq!(
            value["outstanding_blocks"][0]["bytes_sent"],
            bytes.to_string()
        );
        account.outstanding_blocks[0].bytes_sent = 100.into();
        assert!(account.validate().is_err());
    }
}

#[test]
fn ledger_queued_and_failed_copy_sources_are_disjoint_and_keep_order() {
    let mut account = account();
    account.outstanding_blocks.push(BlockAtHalt {
        block_id: source(2, 1),
        content_type: CopyContentType::Pair,
        state: BlockState::ReadyToFile,
        request_id: None,
        bytes_sent: 0.into(),
        frame_bytes: None,
        commit: CommitEvidence::NotRequested,
    });
    let value = decoded(&account);
    assert_eq!(value["outstanding_blocks"][0]["block_id"]["block"], "0");
    assert_eq!(value["outstanding_blocks"][1]["block_id"]["block"], "1");
    assert_eq!(value["outstanding_blocks"][1]["content_type"], "pair");
    assert!(value["outstanding_blocks"][1]["request_id"].is_null());
    assert!(value["outstanding_blocks"][1]["frame_bytes"].is_null());
    for defect in 0..3 {
        let mut changed = account.clone();
        match defect {
            0 => changed
                .outstanding_blocks
                .push(changed.outstanding_blocks[0].clone()),
            1 => changed.queued_copies.push(copy(2)),
            _ => changed.unheld_copy = Some(copy(3)),
        }
        assert!(changed.validate().is_err());
    }
}

#[test]
fn capture_and_pair_progress_never_infer_publication_or_a_terminal_source() {
    let mut account = account();
    let published = StartIdentity {
        seed_id: "seed".into(),
        artifact_id: "artifact".into(),
        dump_sha256: Digest::new("a".repeat(64)).unwrap(),
        config_sha256: Digest::new("b".repeat(64)).unwrap(),
        parent_artifact_id: None,
        position: None,
    };
    account.capture = Some(CaptureAtHalt {
        capture_id: 9.into(),
        state: CaptureState::AwaitPublished,
        commit_sent: true,
        published: None,
    });
    let value = decoded(&account);
    assert_eq!(value["capture"]["capture_id"], "9");
    assert_eq!(value["capture"]["state"], "await_published");
    assert_eq!(value["capture"]["commit_sent"], true);
    assert!(value["capture"]["published"].is_null());
    account.capture.as_mut().unwrap().published = Some(published.clone());
    assert_eq!(
        decoded(&account)["capture"]["published"],
        serde_json::to_value(published).unwrap()
    );
    account.capture.as_mut().unwrap().commit_sent = false;
    assert!(account.validate().is_err());
    account.capture = None;
    account.outstanding_pairs[0].terminal_filed = true;
    assert!(account.validate().is_err());
    account.outstanding_pairs[0].terminal_source = Some(source(6, 0));
    account.validate().unwrap();
}

#[test]
fn first_latch_survives_missing_clock_and_overflow_without_a_fabricated_deadline() {
    for (now, error) in [
        (None, ReportError::ClockUnavailable),
        (Some(u64::MAX.into()), ReportError::CounterOverflow),
    ] {
        let mut account = account();
        account.observed_ns = now;
        let first = cause(&account);
        let mut latch = HaltLatch::new(first.clone(), Some(1));
        assert_eq!(latch.deadline(), Err(error));
        assert_eq!(latch.first(), &first);
        assert_eq!(
            latch.fault_account().unwrap_err(),
            ReportError::SnapshotMissing
        );
        latch.install_account(account).unwrap();
        let value: Value = serde_json::from_str(latch.fault_account().unwrap().get()).unwrap();
        assert_eq!(value["observed_ns"], serde_json::to_value(now).unwrap());
        assert_eq!(latch.deadline(), Err(error));
    }
}

#[test]
fn one_earliest_absolute_deadline_and_first_snapshot_survive_all_later_evidence() {
    let account = account();
    let first = cause(&account);
    assert_eq!(
        HaltLatch::new(first.clone(), None).deadline(),
        Ok(150_000_000_120)
    );
    assert_eq!(
        HaltLatch::new(first.clone(), Some(200_000_000_000)).deadline(),
        Ok(150_000_000_120)
    );
    let mut latch = HaltLatch::new(first.clone(), Some(140));
    assert_eq!(latch.deadline(), Ok(140));
    latch.install_account(account.clone()).unwrap();
    let original = latch.fault_account().unwrap();
    for observed in [Some(130.into()), None] {
        latch
            .append_secondary(SecondaryFailure {
                reason: FlowHaltReason::TraceFailed,
                point: HaltPoint::Teardown,
                observed_ns: observed,
                detail: "later sink loss".into(),
            })
            .unwrap();
    }
    assert_eq!(latch.first(), &first);
    assert_eq!(latch.deadline(), Ok(140));
    assert_eq!(latch.fault_account().unwrap().get(), original.get());
    let rejected = latch.install_account(account).unwrap_err();
    assert_eq!(rejected.error, ReportError::SnapshotAlreadyInstalled);
    assert_eq!(rejected.account.pending_author.unwrap().ticket.0, 17);
    let first_value: Value = serde_json::from_str(original.get()).unwrap();
    let mut unload: Value = serde_json::from_str(latch.unload_account().unwrap().get()).unwrap();
    assert_eq!(unload["secondary"].as_array().unwrap().len(), 2);
    assert_eq!(unload["secondary"][0]["observed_ns"], "130");
    assert!(unload["secondary"][1]["observed_ns"].is_null());
    unload["secondary"] = json!([]);
    assert_eq!(unload, first_value);
}

#[test]
fn a_failed_snapshot_install_returns_its_original_and_cannot_replace_first_cause() {
    let account = account();
    let first = cause(&account);
    let mut latch = HaltLatch::new(first.clone(), None);
    let mut changed = account.clone();
    changed.reason = FlowHaltReason::TraceFailed;
    let failure = latch.install_account(changed).unwrap_err();
    assert_eq!(failure.error, ReportError::FirstCauseMismatch);
    assert_eq!(failure.account.reason, FlowHaltReason::TraceFailed);
    assert_eq!(latch.first(), &first);
    assert!(latch.account().is_none());
    let mut changed = account.clone();
    changed.outstanding_blocks[0].bytes_sent = 100.into();
    let failure = latch.install_account(changed).unwrap_err();
    assert_eq!(failure.account.outstanding_blocks[0].bytes_sent.0, 100);
    assert!(latch.account().is_none());
    latch.install_account(account).unwrap();
}

#[test]
fn escaped_large_snapshot_is_counted_in_full_without_the_control_answer_limit() {
    // A synthetic finite stress shape, not a deployment memory qualification.
    let mut account = account();
    let run = "\u{1}\"\\\n".repeat(4096);
    account.outstanding_blocks = (0..64)
        .map(|sequence| BlockAtHalt {
            block_id: SourceBlockId {
                run: run.clone(),
                sequence: sequence.into(),
                block: 0.into(),
            },
            content_type: CopyContentType::Statement,
            state: BlockState::AwaitingClassification,
            request_id: None,
            bytes_sent: 0.into(),
            frame_bytes: None,
            commit: CommitEvidence::NotRequested,
        })
        .collect();
    account.detail = "escaped \"failure\" \\ \n \t \u{00e9}".repeat(100);
    account.pending_author.as_mut().unwrap().ticket = u64::MAX.into();
    let counted = account.account_bytes().unwrap();
    assert!(counted > weaver_types::state_flow::STATE_ANSWER_LINE_BOUND);
    let raw = account.to_raw().unwrap();
    assert_eq!(counted, raw.get().len());
    let value: Value = serde_json::from_str(raw.get()).unwrap();
    assert_eq!(value["outstanding_blocks"].as_array().unwrap().len(), 64);
    assert_eq!(value["outstanding_blocks"][63]["block_id"]["run"], run);
    assert_eq!(
        value["outstanding_blocks"][63]["block_id"]["sequence"],
        "63"
    );
    assert_eq!(value["pending_author"]["ticket"], u64::MAX.to_string());
    assert_eq!(value["detail"], account.detail);
}

#[test]
fn counting_overflow_and_exact_allocation_failure_refuse_without_truncation() {
    let account = account();
    let mut count = CountWriter {
        bytes: usize::MAX,
        overflow: false,
    };
    assert!(account.write_account(&mut count, &[]).is_err());
    assert!(count.overflow);
    assert_eq!(count.bytes, usize::MAX);
    assert!(matches!(
        ExactWriter::new(usize::MAX),
        Err(ReportError::AllocationFailed)
    ));
    let mut out = ExactWriter::new(2).unwrap();
    out.write_all(b"{}").unwrap();
    assert!(out.write_all(b"x").is_err());
    assert_eq!(out.bytes, b"{}");
    assert_eq!(account.outstanding_blocks.len(), 1);
    assert_eq!(account.pending_author.unwrap().ticket.0, 17);
}
