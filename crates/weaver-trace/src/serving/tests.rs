use std::collections::BTreeSet;

use super::*;
use crate::Sequence;

pub(super) fn partition() -> Partition {
    let mut rules = Vec::new();
    for (id, kind, channels, origins, blocks, context) in [
        (
            "user",
            "message.user",
            "input",
            "operator external",
            "content.text[*]",
            false,
        ),
        (
            "assistant",
            "message.assistant",
            "output",
            "model",
            "content.text[*]",
            false,
        ),
        (
            "loop",
            "loop.instruction",
            "internal",
            "loop",
            "message.content.text[*]",
            false,
        ),
        (
            "pair-start",
            "tool.call.started",
            "tool",
            "model loop",
            "pair",
            false,
        ),
        (
            "pair-end",
            "tool.call.completed",
            "tool",
            "tool loop",
            "pair",
            false,
        ),
        (
            "mark",
            "state.mark",
            "internal",
            "loop",
            "message.content.text[0]",
            true,
        ),
        (
            "plan-input",
            "plan.event",
            "input",
            "operator",
            "message.content.text[*]",
            true,
        ),
        (
            "plan-effect",
            "plan.event",
            "internal",
            "loop",
            "message.content.text[0]",
            true,
        ),
        (
            "outcome",
            "step.outcome",
            "internal",
            "loop",
            "message.content.text[*]",
            true,
        ),
    ] {
        rules.push(PartitionRule {
            id: id.into(),
            kind: kind.into(),
            channels: channels.split(' ').map(str::to_owned).collect(),
            origins: origins.split(' ').map(str::to_owned).collect(),
            blocks: vec![blocks.into()],
            context: context.then(|| "context".into()),
        });
    }
    Partition { rules }
}

fn tee() -> Tee {
    Tee::new(validate_partition(&partition()).unwrap())
}

pub(super) fn source() -> SourceEvent {
    SourceEvent {
        run: "r-1".into(),
        sequence: Sequence(7),
    }
}

pub(super) fn line(kind: Kind, channel: Channel, origin: Origin, payload: &str) -> String {
    let turn = if turn_rule(kind) == TurnRule::Forbidden {
        ""
    } else {
        "\"turn\":\"t-1\","
    };
    format!(
        "{{\"session\":\"s-1\",\"run\":\"r-1\",{turn}\"sequence\":\"7\",\"kind\":\"{}\",\"channel\":\"{}\",\"origin\":\"{}\",\"subsystem\":\"harness\",\"wall_ms\":1,\"monotonic_ns\":\"9007199254740993\",\"payload\":{payload}}}\n",
        kind.as_str(),
        channel.as_str(),
        origin.as_str(),
    )
}

fn statement(outcome: TapOutcome) -> StatementCopy {
    match outcome {
        TapOutcome::Selected(AdmittedCopy::Statement(copy)) => copy,
        other => panic!("expected statement copy, got {other:?}"),
    }
}

fn failure(outcome: TapOutcome, code: TapFailureCode) -> TapFailure {
    match outcome {
        TapOutcome::Failed(failure) => {
            assert_eq!(failure.code, code);
            assert_eq!(failure.source, source());
            failure
        }
        other => panic!("expected failure {code:?}, got {other:?}"),
    }
}

#[test]
fn registry_is_closed_and_covers_every_merged_kind() {
    let expected = [
        "load",
        "unload",
        "session.closed",
        "turn.started",
        "turn.closed",
        "message.system",
        "message.user",
        "message.assistant",
        "message.tool_result",
        "tool.call.started",
        "tool.call.completed",
        "fault",
        "elision",
        "refusal",
        "flush",
        "model.request",
        "model.output",
        "model.measurement",
        "model.field",
        "classify.request",
        "classify.output",
        "recall",
        "message.restored",
        "score",
        "save_point",
        "model.unrecovered_call",
        "tool.return_rejected",
        "loop.instruction",
        "sort.decision",
        "state.sort.measurement",
        "step.outcome",
        "plan.event",
        "plan.break",
        "state.plan.measurement",
        "state.mark",
        "state.mark.position",
        "state.placement",
        "input.offered",
        "state.flow",
        "state.store.measurement",
        "ste100.result",
    ];
    assert_eq!(REGISTRY_ID, "state-copy/1");
    assert_eq!(Kind::ALL.len(), expected.len());
    assert_eq!(
        Kind::ALL
            .iter()
            .map(|k| k.as_str())
            .collect::<BTreeSet<_>>(),
        expected.into_iter().collect()
    );
    for kind in Kind::ALL {
        assert_eq!(Kind::from_name(kind.as_str()), Some(*kind));
        assert_eq!(
            serde_json::to_string(kind).unwrap(),
            format!("\"{}\"", kind.as_str())
        );
        let expected_schema = match *kind {
            Kind::Load => ContentSchema::Head,
            Kind::MessageUser | Kind::MessageAssistant => ContentSchema::Message,
            Kind::ToolCallStarted | Kind::ToolCallCompleted => ContentSchema::Pair,
            Kind::LoopInstruction | Kind::SortDecision => ContentSchema::OneText { context: false },
            Kind::StateMark | Kind::StepOutcome => ContentSchema::OneText { context: true },
            Kind::PlanEvent => ContentSchema::Plan,
            _ => ContentSchema::None,
        };
        assert_eq!(content_schema(*kind), expected_schema, "{kind:?}");
    }
    for name in ["", "*", "message.*", "state.unknown", "message.User"] {
        assert!(Kind::from_name(name).is_none());
    }
    assert_eq!(Channel::ALL.len(), 5);
    assert_eq!(Origin::ALL.len(), 6);
}

#[test]
fn envelope_enforces_every_static_turn_rule_without_reading_payload_meaning() {
    for kind in Kind::ALL {
        let body = line(*kind, Channel::Internal, Origin::Runtime, "{}");
        let with_turn = if body.contains("\"turn\":") {
            body.clone()
        } else {
            body.replace("\"run\":\"r-1\",", "\"run\":\"r-1\",\"turn\":\"t-1\",")
        };
        let without_turn = with_turn.replace("\"turn\":\"t-1\",", "");
        let with = super::record::Record::parse(&source(), &with_turn).is_ok();
        let without = super::record::Record::parse(&source(), &without_turn).is_ok();
        match turn_rule(*kind) {
            TurnRule::Required => assert!(with && !without, "{kind:?}"),
            TurnRule::Forbidden => assert!(!with && without, "{kind:?}"),
            // The source author must prove conditional rules with typed payloads.
            TurnRule::Optional | TurnRule::PayloadDependent => assert!(with && without, "{kind:?}"),
        }
    }
}

#[test]
fn serving_validation_requires_each_call_task_and_input_tuple() {
    let complete = partition();
    validate_partition(&complete).unwrap();
    for index in 0..complete.rules.len() {
        let mut missing = complete.clone();
        missing.rules.remove(index);
        assert!(
            validate_partition(&missing).is_err(),
            "missing rule {index}"
        );
    }
    // Multi-origin rules must cover both sides, not merely have a matching kind.
    for index in [0, 3, 4] {
        for origin in 0..complete.rules[index].origins.len() {
            let mut missing = complete.clone();
            missing.rules[index].origins.remove(origin);
            assert!(
                validate_partition(&missing).is_err(),
                "rule {index}, origin {origin}"
            );
        }
    }
    assert!(validate_partition(&Partition { rules: vec![] }).is_err());
    for index in [0, 1, 6] {
        let mut narrowed = complete.clone();
        narrowed.rules[index].blocks[0] = narrowed.rules[index].blocks[0].replace('*', "0");
        assert!(
            validate_partition(&narrowed).is_err(),
            "partial required wildcard {index}"
        );
    }
}

#[test]
fn every_unselectable_kind_is_refused_at_load() {
    for kind in Kind::ALL {
        if matches!(
            content_schema(*kind),
            ContentSchema::Head | ContentSchema::None
        ) {
            let mut selected = partition();
            selected.rules.push(PartitionRule {
                id: "forbidden".into(),
                kind: kind.as_str().into(),
                channels: vec!["measurement".into()],
                origins: vec!["runtime".into()],
                blocks: vec!["content.text[*]".into()],
                context: None,
            });
            assert_eq!(
                validate_partition(&selected).unwrap_err().code,
                PartitionErrorCode::ForbiddenProjection,
                "{kind:?}"
            );
        }
    }
}

#[test]
fn unknown_vocabulary_duplicates_and_overlapping_rules_refuse() {
    for field in ["kind", "channel", "origin"] {
        let mut bad = partition();
        match field {
            "kind" => bad.rules[0].kind = "*".into(),
            "channel" => bad.rules[0].channels[0] = "*".into(),
            "origin" => bad.rules[0].origins[0] = "*".into(),
            _ => unreachable!(),
        }
        assert_eq!(
            validate_partition(&bad).unwrap_err().code,
            PartitionErrorCode::UnknownVocabulary
        );
    }
    for field in ["id", "channels", "origins", "tuple"] {
        let mut bad = partition();
        match field {
            "id" => bad.rules[1].id = bad.rules[0].id.clone(),
            "channels" => bad.rules[0].channels.push("input".into()),
            "origins" => bad.rules[0].origins.push("operator".into()),
            "tuple" => {
                let mut repeated = bad.rules[0].clone();
                repeated.id = "overlap".into();
                bad.rules.push(repeated);
            }
            _ => unreachable!(),
        }
        assert!(validate_partition(&bad).is_err(), "{field}");
    }
}

#[test]
fn projections_are_finite_whole_and_nonoverlapping() {
    for paths in [
        vec![],
        vec!["content.text[*]", "content.text[0]"],
        vec!["content.text[*]", "content.text[*]"],
        vec!["content.text[01]"],
        vec!["content.text[-1]"],
        vec!["content.text[18446744073709551616]"],
        vec!["content.text[0].text"],
        vec!["content[0]"],
        vec!["payload.*"],
        vec!["message.content.text[*]"],
        vec!["pair", "content.text[*]"],
    ] {
        let mut bad = partition();
        bad.rules[0].blocks = paths.iter().map(|p| (*p).into()).collect();
        assert!(validate_partition(&bad).is_err(), "{paths:?}");
    }
    for index in [2, 5, 7, 8] {
        let mut bad = partition();
        bad.rules[index].blocks = vec!["message.content.text[1]".into()];
        assert!(
            validate_partition(&bad).is_err(),
            "single-Text index {index}"
        );
    }
    let mut bad = partition();
    bad.rules[3].blocks.push("pair".into());
    assert!(validate_partition(&bad).is_err());
}

#[test]
fn context_is_required_only_on_licensed_projection_surfaces() {
    for index in 0..partition().rules.len() {
        let mut bad = partition();
        bad.rules[index].context = match bad.rules[index].context {
            Some(_) => None,
            None => Some("context".into()),
        };
        assert!(validate_partition(&bad).is_err(), "context on rule {index}");
    }
    let mut bad = partition();
    bad.rules[5].context = Some("context.effect".into());
    assert!(validate_partition(&bad).is_err());
}

#[test]
fn every_context_rule_selects_original_block_zero_even_outside_required_tuples() {
    let mut declaration = partition();
    declaration.rules.push(PartitionRule {
        id: "extra-plan".into(),
        kind: "plan.event".into(),
        channels: vec!["input".into()],
        origins: vec!["external".into()],
        blocks: vec!["message.content.text[1]".into()],
        context: Some("context".into()),
    });
    let failure = validate_partition(&declaration).unwrap_err();
    assert_eq!(failure.code, PartitionErrorCode::InvalidProjection);
    assert_eq!(failure.rule_id.as_deref(), Some("extra-plan"));
    assert_eq!(failure.path.as_deref(), Some("blocks"));

    for projection in ["message.content.text[0]", "message.content.text[*]"] {
        declaration.rules.last_mut().unwrap().blocks = vec![projection.into()];
        assert!(validate_partition(&declaration).is_ok(), "{projection}");
    }
}

#[test]
fn mixed_message_copies_whole_text_in_original_order_and_preserves_trace_bytes() {
    let first = r#"{"type":"text","text":"Read caf\u00e9.\nKeep \"quotes\"."}"#;
    let last = r#"{"type":"text","text":"Write α."}"#;
    let payload = format!(
        "{{\"role\":\"assistant\",\"content\":[{first},{{\"type\":\"tool_call\",\"name\":\"sh\",\"arguments\":\"echo x\"}},{{\"type\":\"tool_result\",\"content\":\"x\"}},{{\"type\":\"unrecovered_call\",\"fragment\":\"<tool>\"}},{last}]}}"
    );
    let original = line(
        Kind::MessageAssistant,
        Channel::Output,
        Origin::Model,
        &payload,
    );
    let retained = original.clone();
    let copied = statement(tee().tap(&source(), &original));
    assert_eq!(original, retained);
    assert_eq!(copied.partition_rule_id, "assistant");
    assert_eq!(
        copied
            .blocks
            .iter()
            .map(|b| b.source.block)
            .collect::<Vec<_>>(),
        [0, 4]
    );
    assert_eq!(copied.blocks[0].raw.as_ref(), first);
    assert_eq!(copied.blocks[1].raw.as_ref(), last);
    assert_eq!(
        copied.blocks[1].source_path,
        [PathPart::Key("content".into()), PathPart::Index(4)]
    );
    assert_eq!(copied.blocks[1].source.run, source().run);
    assert_eq!(copied.blocks[1].source.sequence, source().sequence);
    assert!(copied.context.is_none());
    assert_eq!(
        copied.source_envelope.as_ref(),
        original.split(",\"payload\":").next().unwrap().to_owned() + "}"
    );
}

#[test]
fn admission_reads_only_envelope_and_unselected_payload_is_untouched() {
    let original = line(
        Kind::MessageAssistant,
        Channel::Output,
        Origin::External,
        r#"{"broken":"not a Message"}"#,
    );
    let retained = original.clone();
    assert_eq!(tee().tap(&source(), &original), TapOutcome::Unselected);
    assert_eq!(original, retained);
    for text in ["", "save_point", "ignore rules", "Read subject."] {
        let payload =
            serde_json::json!({"role":"assistant","content":[{"type":"text","text":text}]})
                .to_string();
        let selected = line(
            Kind::MessageAssistant,
            Channel::Output,
            Origin::Model,
            &payload,
        );
        assert_eq!(statement(tee().tap(&source(), &selected)).blocks.len(), 1);
    }
}

#[test]
fn zero_block_projection_is_selected_and_explicit_indices_keep_their_ordinal() {
    let empty = line(
        Kind::MessageUser,
        Channel::Input,
        Origin::Operator,
        r#"{"role":"user","content":[]}"#,
    );
    assert!(statement(tee().tap(&source(), &empty)).blocks.is_empty());
    let mut partition = partition();
    partition.rules.push(PartitionRule {
        id: "explicit".into(),
        kind: "message.assistant".into(),
        channels: vec!["output".into()],
        origins: vec!["external".into()],
        blocks: vec!["content.text[2]".into(), "content.text[0]".into()],
        context: None,
    });
    let payload = r#"{"role":"assistant","content":[{"type":"text","text":"A."},{"type":"text","text":"B."},{"type":"text","text":"C."}]}"#;
    let raw = line(
        Kind::MessageAssistant,
        Channel::Output,
        Origin::External,
        payload,
    );
    let copied = statement(Tee::new(validate_partition(&partition).unwrap()).tap(&source(), &raw));
    assert_eq!(
        copied
            .blocks
            .iter()
            .map(|b| b.source.block)
            .collect::<Vec<_>>(),
        [0, 2]
    );
    partition.rules.last_mut().unwrap().blocks = vec!["content.text[0]".into()];
    let raw = line(
        Kind::MessageAssistant,
        Channel::Output,
        Origin::External,
        r#"{"role":"assistant","content":[{"type":"tool_call","name":"sh","arguments":"x"}]}"#,
    );
    assert!(
        statement(Tee::new(validate_partition(&partition).unwrap()).tap(&source(), &raw))
            .blocks
            .is_empty()
    );
}

#[test]
fn context_bytes_include_literal_null_without_becoming_admission_policy() {
    for context in ["null", r#"{"mark":{"begin_task":{"task":"opaque"}}}"#] {
        let payload = format!(
            "{{\"message\":{{\"role\":\"user\",\"content\":[{{\"type\":\"text\",\"text\":\"Record outcome.\"}}]}},\"context\":{context},\"template_id\":\"outcome\"}}"
        );
        let raw = line(Kind::StepOutcome, Channel::Internal, Origin::Loop, &payload);
        let copy = statement(tee().tap(&source(), &raw));
        assert_eq!(
            copy.blocks[0].source_path,
            [
                PathPart::Key("message".into()),
                PathPart::Key("content".into()),
                PathPart::Index(0)
            ]
        );
        assert_eq!(copy.context.as_ref().unwrap().raw.as_ref(), context);
        assert_eq!(
            copy.context.unwrap().source_path,
            [PathPart::Key("context".into())]
        );
    }
}

#[test]
fn plan_projection_preserves_multiple_original_blocks_and_exact_sidecar() {
    let payload = r#"{"message":{"role":"user","content":[{"type":"text","text":"Read A."},{"type":"text","text":"Read B."}]},"context":{"input_id":"18"},"document":"opaque plan document"}"#;
    let raw = line(Kind::PlanEvent, Channel::Input, Origin::Operator, payload)
        .replace("\"turn\":\"t-1\",", "");
    let copy = statement(tee().tap(&source(), &raw));
    assert_eq!(
        copy.blocks
            .iter()
            .map(|block| block.source.block)
            .collect::<Vec<_>>(),
        [0, 1]
    );
    assert_eq!(copy.context.unwrap().raw.as_ref(), r#"{"input_id":"18"}"#);
    assert!(!copy.source_envelope.contains("turn"));
}

#[test]
fn canonical_envelope_copy_keeps_escaped_identity_and_full_width_counters() {
    let raw = line(
        Kind::MessageUser,
        Channel::Input,
        Origin::Operator,
        r#"{"role":"user","content":[{"type":"text","text":"Read x."}]}"#,
    )
    .replace("\"session\":\"s-1\"", "\"session\":\"s\\\"\\nα\"")
    .replace(
        "\"sequence\":\"7\"",
        "\"sequence\":\"18446744073709551615\"",
    )
    .replace(
        "\"wall_ms\":1",
        "\"causal_parent\":\"9007199254740993\",\"wall_ms\":1",
    );
    let source = SourceEvent {
        run: "r-1".into(),
        sequence: Sequence(u64::MAX),
    };
    let copy = statement(tee().tap(&source, &raw));
    assert_eq!(copy.blocks[0].source.sequence, Sequence(u64::MAX));
    assert_eq!(
        copy.source_envelope.as_ref(),
        raw.split(",\"payload\":").next().unwrap().to_owned() + "}"
    );
}

#[test]
fn pair_copy_is_the_complete_payload_at_virtual_block_zero() {
    let payload = r#"{"call_id":{"run":"r-1","sequence":"6","call":"0"},"call_block":"1","statement":{"block_id":{"run":"r-1","sequence":"6","block":"0"},"binding":"0"},"lane":"external","request":{"external":{"name":"sh","arguments":"echo café"}},"retry_of":null}"#;
    let raw = line(Kind::ToolCallStarted, Channel::Tool, Origin::Model, payload);
    let original = raw.clone();
    match tee().tap(&source(), &raw) {
        TapOutcome::Selected(AdmittedCopy::Pair(pair)) => {
            assert_eq!(pair.block.raw.as_ref(), payload);
            assert_eq!(pair.block.source.block, 0);
            assert!(pair.block.source_path.is_empty());
            assert_eq!(pair.partition_rule_id, "pair-start");
        }
        other => panic!("{other:?}"),
    }
    assert_eq!(raw, original);
}

#[test]
fn missing_or_malformed_selected_data_fails_and_never_becomes_unselected() {
    for (payload, expected) in [
        (r#"{"role":"user"}"#, TapFailureCode::MissingProjection),
        (
            r#"{"role":"user","content":null}"#,
            TapFailureCode::InvalidProjection,
        ),
        (
            r#"{"role":"user","content":[{"type":"text"}]}"#,
            TapFailureCode::InvalidProjection,
        ),
        (
            r#"{"role":"user","content":[{"type":"text","text":1}]}"#,
            TapFailureCode::InvalidProjection,
        ),
        (
            r#"{"role":"user","content":[{"type":"unknown","text":"x"}]}"#,
            TapFailureCode::InvalidProjection,
        ),
        (
            r#"{"role":"user","content":[{"type":"tool_call","name":"sh"}]}"#,
            TapFailureCode::InvalidProjection,
        ),
        (
            r#"{"role":"user","content":[{"type":"text","text":"x","text":"y"}]}"#,
            TapFailureCode::InvalidProjection,
        ),
    ] {
        let raw = line(Kind::MessageUser, Channel::Input, Origin::Operator, payload);
        failure(tee().tap(&source(), &raw), expected);
    }
    let missing_context = line(
        Kind::StateMark,
        Channel::Internal,
        Origin::Loop,
        r#"{"message":{"role":"user","content":[{"type":"text","text":"Mark task."}]}}"#,
    );
    assert_eq!(
        failure(
            tee().tap(&source(), &missing_context),
            TapFailureCode::MissingProjection
        )
        .source_path,
        Some(vec![PathPart::Key("context".into())])
    );
    let missing_index = line(
        Kind::StateMark,
        Channel::Internal,
        Origin::Loop,
        r#"{"message":{"role":"user","content":[]},"context":null}"#,
    );
    failure(
        tee().tap(&source(), &missing_index),
        TapFailureCode::MissingProjection,
    );
}

#[test]
fn malformed_canonical_envelopes_refuse_instead_of_losing_attribution() {
    let good = line(
        Kind::MessageUser,
        Channel::Input,
        Origin::Operator,
        r#"{"role":"user","content":[{"type":"text","text":"Read x."}]}"#,
    );
    for bad in [
        good.replace("\"sequence\":\"7\"", "\"sequence\":7"),
        good.replace("\"sequence\":\"7\"", "\"sequence\":\"07\""),
        good.replace("\"sequence\":\"7\"", "\"sequence\":\"8\""),
        good.replace("\"run\":\"r-1\"", "\"run\":\"foreign\""),
        good.replace("\"channel\":\"input\",", ""),
        good.replace("\"origin\":\"operator\"", "\"origin\":\"unknown\""),
        good.replace(
            "\"origin\":\"operator\"",
            "\"origin\":\"operator\",\"origin\":\"model\"",
        ),
        good.replace("\"turn\":\"t-1\"", "\"turn\":null"),
        good.replace("\"turn\":\"t-1\",", ""),
        good.replace("\"wall_ms\":1", "\"causal_parent\":null,\"wall_ms\":1"),
        good.replace("\"wall_ms\":1", "\"wall_ms\":\"1\""),
        good.replace("\"kind\":\"message.user\"", "\"kind\":\"unknown\""),
        good.replace("\"payload\":", "\"extra\":0,\"payload\":"),
        format!("{good}\n"),
    ] {
        failure(
            tee().tap(&source(), &bad),
            TapFailureCode::InvalidCanonicalRecord,
        );
    }
}

#[test]
fn head_is_untapped_and_failure_or_explicit_stop_is_terminal() {
    let head = line(Kind::Load, Channel::Internal, Origin::Runtime, "{}");
    assert_eq!(tee().tap(&source(), &head), TapOutcome::HeadUntapped);
    let input = line(
        Kind::MessageUser,
        Channel::Input,
        Origin::Operator,
        r#"{"role":"user","content":[{"type":"text","text":"Read x."}]}"#,
    );
    let mut stopped = tee();
    stopped.stop();
    stopped.stop();
    assert_eq!(stopped.tap(&source(), &input), TapOutcome::Stopped);
    let mut failed = tee();
    failure(
        failed.tap(&source(), "not JSON"),
        TapFailureCode::InvalidCanonicalRecord,
    );
    assert_eq!(failed.tap(&source(), &input), TapOutcome::Stopped);
    assert_eq!(failed.tap(&source(), &head), TapOutcome::Stopped);
}
