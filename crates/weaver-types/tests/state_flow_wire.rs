//! Shared complete-frame corpus for future state and harness endpoint implementations.

use serde_json::value::RawValue;
use weaver_types::state_flow::*;

fn common(id: u64) -> Common {
    Common {
        session_id: "session".into(),
        run_id: "run".into(),
        request_id: id.into(),
    }
}

fn digest() -> Digest {
    Digest::new("a".repeat(64)).unwrap()
}

fn start() -> StartIdentity {
    StartIdentity {
        seed_id: "1".repeat(32),
        artifact_id: "2".repeat(32),
        dump_sha256: digest(),
        config_sha256: digest(),
        parent_artifact_id: None,
        position: None,
    }
}

fn source(sequence: u64, kind: &str) -> SourceEnvelope {
    SourceEnvelope {
        session: "session".into(),
        run: "run".into(),
        turn: None,
        sequence: sequence.into(),
        kind: kind.into(),
        channel: "output".into(),
        origin: "model".into(),
        subsystem: "harness".into(),
        causal_parent: None,
        wall_ms: 0,
        monotonic_ns: 1.into(),
    }
}

fn block_id(sequence: u64) -> SourceBlockId {
    SourceBlockId {
        run: "run".into(),
        sequence: sequence.into(),
        block: 0.into(),
    }
}

fn span(start: u64, end: u64) -> Span {
    Span {
        start: start.into(),
        end: end.into(),
    }
}

fn bindings() -> Vec<Binding> {
    vec![Binding {
        statement_span: span(0, 14),
        verb: "read".into(),
        verb_span: span(0, 4),
        condition: None,
        condition_span: None,
        nouns: vec![NounBinding {
            value: "file".into(),
            span: span(9, 13),
        }],
    }]
}

const RAW_TEXT: &str = r#"{ "type" : "text", "text" : "Read the \u0066ile." }"#;

fn filing() -> StatementFile {
    StatementFile {
        block_id: block_id(2),
        source: source(2, "message.assistant"),
        partition_rule_id: "statements".into(),
        source_path: vec![
            SourcePathPart::Key("content".into()),
            SourcePathPart::Index { index: 0.into() },
        ],
        admission: Admission::Admitted,
        raw_block: RawValue::from_string(RAW_TEXT.into()).unwrap(),
        text: "Read the file.".into(),
        scheme_id: "scheme".into(),
        bindings: bindings(),
        context: None,
        effect: None,
    }
}

fn pair_filing() -> PairFile {
    let record = PairStarted {
        call_id: CallId {
            run: "run".into(),
            sequence: 2.into(),
            call: 0.into(),
        },
        call_block: 0.into(),
        statement: StatementRef {
            block_id: block_id(2),
            binding: 0.into(),
        },
        lane: PairLane::Recall,
        request: PairRequest::Recall {
            subject: "file".into(),
        },
        retry_of: None,
    };
    PairFile {
        block_id: block_id(3),
        source: source(3, "pair.started"),
        partition_rule_id: "pairs".into(),
        source_path: vec![],
        admission: Admission::Admitted,
        raw_block: RawValue::from_string(serde_json::to_string(&record).unwrap()).unwrap(),
        scheme_id: "scheme".into(),
        pair: PairRecord::Started(record),
    }
}

fn observation(
    request: u64,
    operation: StateOperation,
    operation_id: u64,
) -> StateOperationObservation {
    StateOperationObservation {
        operation_id: operation_id.into(),
        parent_id: None,
        request_id: request.into(),
        operation,
        began_ns: 10.into(),
        ended_ns: 20.into(),
        phases: vec![
            OperationPhase {
                name: PhaseName::Validate,
                began_ns: 10.into(),
                ended_ns: 11.into(),
            },
            OperationPhase {
                name: PhaseName::Encode,
                began_ns: 19.into(),
                ended_ns: 20.into(),
            },
        ],
        resource: OperationResource {
            cpu_ns: None,
            copied_bytes: Some(0.into()),
            held_bytes: Some(0.into()),
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

fn member() -> MemberObservationIdentity {
    MemberObservationIdentity {
        instrument_version: "state-observation/1".into(),
        pid: 1,
        start_ticks: None,
        ticks_per_second: None,
        unavailable_reason: Some(ObservationUnavailable::NotSampled),
    }
}

fn read_limits() -> ReadLimits {
    ReadLimits {
        max_result_bytes: 1024.into(),
        measurement_ref: "fixture:read-limit".into(),
    }
}

fn open() -> StateFrame {
    StateFrame::Open {
        common: common(1),
        schema_id: STATE_CONTENT_SCHEMA.into(),
        scheme_id: "scheme".into(),
        start: start(),
        read_limits: read_limits(),
    }
}

fn opened() -> StateFrame {
    StateFrame::Opened {
        common: common(1),
        schema_id: STATE_CONTENT_SCHEMA.into(),
        scheme_id: "scheme".into(),
        start: start(),
        read_limits: read_limits(),
        member_observation: member(),
        observation: observation(1, StateOperation::Open, 1),
    }
}

fn seed_request() -> SeedRequest {
    SeedRequest {
        schema_id: STATE_CONTENT_SCHEMA.into(),
        seed_id: "1".repeat(32),
        scheme_id: "scheme".into(),
        // This codec fixture does not certify or execute a semantic artifact.
        ste100_scheme: "{}".into(),
        checker_artifact_sha256: digest(),
        prompt_text: "Read the file.".into(),
        raw_block: RawValue::from_string(RAW_TEXT.into()).unwrap(),
        bindings: bindings(),
        certificate: SeedCertificate {
            prompt_sha256: digest(),
            scheme_id: "scheme".into(),
            checker_artifact_sha256: digest(),
            review_reference: "fixture:review".into(),
            bindings: bindings(),
            duties: vec![
                PromptDuty::Ste100Emission,
                PromptDuty::Recall,
                PromptDuty::ForwardFlags,
                PromptDuty::LockedPlan,
                PromptDuty::ReviewClosure,
                PromptDuty::Restatement,
                PromptDuty::Ste100InputHistory,
                PromptDuty::InitialPlan,
                PromptDuty::WallSummary,
            ],
        },
    }
}

#[test]
fn all_eight_serving_variants_round_trip_without_normalizing_raw_bytes() {
    let frames = [
        open(),
        opened(),
        StateFrame::File {
            common: common(2),
            filing: filing(),
        },
        StateFrame::FilePair {
            common: common(3),
            filing: pair_filing(),
        },
        StateFrame::Filed {
            common: common(2),
            block_id: block_id(2),
            observation: observation(2, StateOperation::File, 2),
        },
        StateFrame::Refused {
            common: common(2),
            code: StateRefusalCode::InvalidBinding,
            detail: "span".into(),
            block_id: Some(block_id(2)),
            observation: observation(2, StateOperation::File, 2),
        },
        StateFrame::Control {
            common: common(4),
            frame: StateControl::Held {},
        },
        StateFrame::ControlAnswer {
            common: common(4),
            frame: StateControlAnswer::Held { count: 1.into() },
            observation: observation(4, StateOperation::Held, 4),
        },
    ];
    let mut ops = std::collections::BTreeSet::new();
    for frame in frames {
        let direction = if frame.is_request() {
            FrameDirection::Request
        } else {
            FrameDirection::Response
        };
        let encoded = encode_state_line(&frame, direction).unwrap();
        let decoded = decode_state_line(&encoded, direction).unwrap();
        assert_eq!(encode_state_line(&decoded, direction).unwrap(), encoded);
        assert_eq!(decoded.op(), frame.op());
        assert!(ops.insert(decoded.op()));
        if let StateFrame::File { filing, .. } = decoded {
            assert_eq!(filing.raw_block.get(), RAW_TEXT);
            filing.validate_structure(&["read".into()]).unwrap();
        }
    }
    assert_eq!(ops.len(), 8);
}

#[test]
fn every_common_and_variant_key_is_required_and_no_unknown_key_is_ignored() {
    let frame = serde_json::to_string(&open()).unwrap();
    for field in [
        "protocol",
        "session_id",
        "run_id",
        "request_id",
        "op",
        "schema_id",
        "scheme_id",
        "start",
        "read_limits",
    ] {
        // Raw maps are test mutations only. They never buffer production decoding.
        let mut parts: std::collections::BTreeMap<String, Box<RawValue>> =
            serde_json::from_str(&frame).unwrap();
        parts.remove(field);
        assert!(
            serde_json::from_str::<StateFrame>(&serde_json::to_string(&parts).unwrap()).is_err(),
            "{field}"
        );
    }
    for bad in [
        frame.replace("state-flow/1", "state-flow/2"),
        frame.replace("\"op\":\"open\"", "\"op\":\"unknown\""),
        frame.replace("\"request_id\":\"1\"", "\"request_id\":\"2\""),
        frame.replace("\"request_id\":\"1\"", "\"request_id\":\"01\""),
        frame.replace("\"request_id\":\"1\"", "\"request_id\":1"),
        frame.replace(
            "\"request_id\":\"1\"",
            "\"request_id\":\"18446744073709551616\"",
        ),
        frame.replacen('{', "{\"observation\":null,", 1),
        frame.replacen('{', "{\"session_id\":\"session\",", 1),
        frame.replacen('{', "{\"session_\\u0069d\":\"session\",", 1),
        format!("{frame} {{}}"),
    ] {
        assert!(serde_json::from_str::<StateFrame>(&bad).is_err(), "{bad}");
    }
    // Unsupported schema and invalid read limits are typed operation refusals.
    assert!(
        serde_json::from_str::<StateFrame>(&frame.replace(STATE_CONTENT_SCHEMA, "unknown")).is_ok()
    );
    assert!(
        serde_json::from_str::<StateFrame>(&frame.replace(
            "\"max_result_bytes\":\"1024\"",
            "\"max_result_bytes\":\"0\""
        ))
        .is_ok()
    );
}

#[test]
fn opaque_blocks_and_nested_control_objects_still_refuse_duplicate_keys() {
    let file = StateFrame::File {
        common: common(2),
        filing: filing(),
    };
    let encoded = serde_json::to_string(&file).unwrap();
    let duplicate = encoded.replace(
        RAW_TEXT,
        r#"{"type":"text","text":"Read the file.","te\u0078t":"changed"}"#,
    );
    assert!(serde_json::from_str::<StateFrame>(&duplicate).is_err());
    let pair = StateFrame::FilePair {
        common: common(3),
        filing: pair_filing(),
    };
    let duplicate_pair = serde_json::to_string(&pair).unwrap().replacen(
        "\"raw_block\":{",
        "\"raw_block\":{\"retry_of\":null,",
        1,
    );
    assert!(serde_json::from_str::<StateFrame>(&duplicate_pair).is_err());
    let control = r#"{"protocol":"state-flow/1","session_id":"session","run_id":"run","request_id":"2","op":"control","frame":{"read":{"selector":{"history":{"noun":"file","noun":"other","verb":null,"condition":null}}}}}"#;
    assert!(serde_json::from_str::<StateFrame>(control).is_err());
    assert!(
        serde_json::from_str::<StateFrame>(
            &control.replace("\"noun\":\"other\",", "\"extra\":null,")
        )
        .is_err()
    );
}

#[test]
fn every_response_requires_observation_and_every_request_refuses_it() {
    let frames = [
        opened(),
        StateFrame::Filed {
            common: common(2),
            block_id: block_id(2),
            observation: observation(2, StateOperation::File, 2),
        },
        StateFrame::Refused {
            common: common(1),
            code: StateRefusalCode::OpenFailed,
            detail: "unavailable".into(),
            block_id: None,
            observation: observation(1, StateOperation::Open, 1),
        },
        StateFrame::ControlAnswer {
            common: common(2),
            frame: StateControlAnswer::Held { count: 0.into() },
            observation: observation(2, StateOperation::Held, 2),
        },
    ];
    for frame in frames {
        let mut parts: std::collections::BTreeMap<String, Box<RawValue>> =
            serde_json::from_str(&serde_json::to_string(&frame).unwrap()).unwrap();
        parts.remove("observation");
        assert!(
            serde_json::from_str::<StateFrame>(&serde_json::to_string(&parts).unwrap()).is_err(),
            "{}",
            frame.op()
        );
    }
    let refused = serde_json::to_string(&StateFrame::Refused {
        common: common(1),
        code: StateRefusalCode::OpenFailed,
        detail: "failed".into(),
        block_id: None,
        observation: observation(1, StateOperation::Open, 1),
    })
    .unwrap();
    assert!(
        serde_json::from_str::<StateFrame>(&refused.replace("\"block_id\":null,", "")).is_err()
    );
    assert!(
        serde_json::from_str::<StateFrame>(&serde_json::to_string(&open()).unwrap().replacen(
            '{',
            "{\"observation\":null,",
            1
        ))
        .is_err()
    );
}

#[test]
fn response_validation_checks_all_echoes_refusal_vocabularies_and_observation_identity() {
    opened().validate_response_to(&open(), 1.into()).unwrap();
    let original = opened();
    let mut mutations = Vec::new();
    for slot in 0..8 {
        let mut changed = original.clone();
        if let StateFrame::Opened {
            common,
            schema_id,
            scheme_id,
            start,
            read_limits,
            member_observation,
            observation,
        } = &mut changed
        {
            match slot {
                0 => common.run_id = "other".into(),
                1 => common.session_id = "other".into(),
                2 => *schema_id = "other".into(),
                3 => *scheme_id = "other".into(),
                4 => start.artifact_id = "other".into(),
                5 => read_limits.max_result_bytes = 9.into(),
                6 => member_observation.pid = 0,
                7 => observation.operation_id = 2.into(),
                _ => unreachable!(),
            }
        }
        mutations.push(changed);
    }
    for changed in mutations {
        assert!(changed.validate_response_to(&open(), 1.into()).is_err());
    }
    let request = StateFrame::File {
        common: common(2),
        filing: filing(),
    };
    let mut answer = StateFrame::Filed {
        common: common(2),
        block_id: block_id(2),
        observation: observation(2, StateOperation::File, 2),
    };
    answer.validate_response_to(&request, 2.into()).unwrap();
    if let StateFrame::Filed { block_id, .. } = &mut answer {
        block_id.block = 1.into();
    }
    assert!(answer.validate_response_to(&request, 2.into()).is_err());
    for (code, allowed) in [
        (StateRefusalCode::WriteFailed, true),
        (StateRefusalCode::InvalidBinding, true),
        (StateRefusalCode::UnknownPair, false),
        (StateRefusalCode::OpenFailed, false),
    ] {
        let refused = StateFrame::Refused {
            common: common(2),
            code,
            detail: "failed".into(),
            block_id: Some(block_id(2)),
            observation: observation(2, StateOperation::File, 2),
        };
        assert_eq!(
            refused.validate_response_to(&request, 2.into()).is_ok(),
            allowed
        );
    }
    let mut wrong = StateFrame::Filed {
        common: common(2),
        block_id: block_id(2),
        observation: observation(2, StateOperation::File, 2),
    };
    for slot in 0..4 {
        if let StateFrame::Filed {
            observation: observed,
            ..
        } = &mut wrong
        {
            *observed = observation(2, StateOperation::File, 2);
            match slot {
                0 => observed.request_id = 3.into(),
                1 => observed.operation = StateOperation::Read,
                2 => observed.operation_id = 3.into(),
                3 => observed.ended_ns = 0.into(),
                _ => unreachable!(),
            }
        }
        assert!(wrong.validate_response_to(&request, 2.into()).is_err());
    }
}

#[test]
fn controls_cover_every_operation_and_capture_continuations_reuse_only_exchange_id() {
    let page = ReadPage {
        snapshot_id: 1.into(),
        offset: 0.into(),
        total_bytes: 2.into(),
        octets: "e30=".into(),
        done: true,
    };
    let cases = [
        (
            StateControl::Read {
                selector: ReadSelector::SystemPrompt {},
            },
            StateControlAnswer::ReadPage(page.clone()),
        ),
        (
            StateControl::Next {
                snapshot_id: 1.into(),
                offset: 0.into(),
            },
            StateControlAnswer::ReadPage(page),
        ),
        (
            StateControl::Release {
                snapshot_id: 1.into(),
            },
            StateControlAnswer::Released {
                snapshot_id: 1.into(),
            },
        ),
        (StateControl::Shape {}, StateControlAnswer::Shape(vec![])),
        (
            StateControl::Held {},
            StateControlAnswer::Held { count: 0.into() },
        ),
        (
            StateControl::Position {},
            StateControlAnswer::Position(None),
        ),
        (
            StateControl::Grants {},
            StateControlAnswer::Grants {
                surface: vec!["sh".into()],
            },
        ),
        (
            StateControl::Snapshot {
                capture_id: 7.into(),
            },
            StateControlAnswer::Snapshot {
                capture_id: 7.into(),
                room_name: "capture".into(),
                size_bytes: 32.into(),
                position: None,
            },
        ),
        (
            StateControl::Acknowledge {
                capture_id: 7.into(),
                dump_sha256: digest(),
            },
            StateControlAnswer::Finished {
                capture_id: 7.into(),
                room_name: "capture".into(),
            },
        ),
        (
            StateControl::Abort {
                capture_id: 7.into(),
            },
            StateControlAnswer::Aborted {
                capture_id: 7.into(),
            },
        ),
        (
            StateControl::ReleaseCapture {
                capture_id: 7.into(),
            },
            StateControlAnswer::CaptureReleased {
                capture_id: 7.into(),
            },
        ),
    ];
    for (index, (request, answer)) in cases.into_iter().enumerate() {
        let operation = request.operation();
        let request = StateFrame::Control {
            common: common(8),
            frame: request,
        };
        let answer = StateFrame::ControlAnswer {
            common: common(8),
            frame: answer,
            observation: observation(8, operation, index as u64 + 1),
        };
        let decoded = decode_state_line(
            &encode_state_line(&answer, FrameDirection::Response).unwrap(),
            FrameDirection::Response,
        )
        .unwrap();
        decoded
            .validate_response_to(&request, (index as u64 + 1).into())
            .unwrap();
        assert!(
            decoded
                .validate_response_to(&request, (index as u64 + 2).into())
                .is_err()
        );
        let wrong = StateFrame::Control {
            common: common(8),
            frame: StateControl::Abort {
                capture_id: 99.into(),
            },
        };
        assert!(
            decoded
                .validate_response_to(&wrong, (index as u64 + 1).into())
                .is_err()
        );
    }
    let refusal = StateControlAnswer::ControlRefused {
        code: ControlRefusalCode::Capture(CaptureRefusalCode::InvalidCaptureState),
        detail: "capture held".into(),
    };
    refusal
        .validate_for(&StateControl::Read {
            selector: ReadSelector::SystemPrompt {},
        })
        .unwrap();
}

#[test]
fn all_seed_variants_are_strict_and_cannot_carry_session_state() {
    let request = seed_request();
    let frames = [
        SeedFrame::Seed {
            request: Box::new(request.clone()),
        },
        SeedFrame::Created {
            created: SeedCreated {
                schema_id: STATE_CONTENT_SCHEMA.into(),
                seed_id: request.seed_id.clone(),
                room_name: "seed".into(),
                size_bytes: 42.into(),
            },
        },
        SeedFrame::Refused {
            code: SeedRefusal::MalformedSeed,
            detail: "text".into(),
        },
    ];
    for frame in frames {
        let direction = if frame.is_request() {
            FrameDirection::Request
        } else {
            FrameDirection::Response
        };
        let bytes = encode_seed_line(&frame, direction).unwrap();
        let result = decode_seed_line(&bytes, direction).unwrap();
        assert_eq!(encode_seed_line(&result, direction).unwrap(), bytes);
        let text = String::from_utf8(bytes).unwrap();
        for bad in [
            text.replace("\"request_id\":\"1\"", "\"request_id\":\"2\""),
            text.replacen('{', "{\"run_id\":\"run\",", 1),
            text.replacen('{', "{\"observation\":null,", 1),
            text.replace("state-seed/1", "state-flow/1"),
        ] {
            assert!(serde_json::from_str::<SeedFrame>(&bad).is_err());
        }
        if let SeedFrame::Seed { request } = result {
            assert_eq!(request.raw_block.get(), RAW_TEXT);
        }
    }
    let duplicate = serde_json::to_string(&SeedFrame::Seed {
        request: Box::new(request),
    })
    .unwrap()
    .replace(
        RAW_TEXT,
        r#"{"type":"text","text":"first","text":"second"}"#,
    );
    assert!(serde_json::from_str::<SeedFrame>(&duplicate).is_err());
}

#[test]
fn full_line_bounds_count_wrapper_and_lf_and_wrong_direction_refuses() {
    let mut response = SeedFrame::Refused {
        code: SeedRefusal::MalformedSeed,
        detail: String::new(),
    };
    let overhead = encode_seed_line(&response, FrameDirection::Response)
        .unwrap()
        .len();
    if let SeedFrame::Refused { detail, .. } = &mut response {
        *detail = "a".repeat(STATE_ANSWER_LINE_BOUND - overhead);
    }
    let at_bound = encode_seed_line(&response, FrameDirection::Response).unwrap();
    assert_eq!(at_bound.len(), STATE_ANSWER_LINE_BOUND);
    decode_seed_line(&at_bound, FrameDirection::Response).unwrap();
    if let SeedFrame::Refused { detail, .. } = &mut response {
        detail.push('a');
    }
    assert!(matches!(
        encode_seed_line(&response, FrameDirection::Response),
        Err(FrameCodecError::FrameTooLarge)
    ));
    let mut over = at_bound.clone();
    over.insert(over.len() - 1, b' ');
    assert!(matches!(
        decode_seed_line(&over, FrameDirection::Response),
        Err(FrameCodecError::FrameTooLarge)
    ));
    assert!(matches!(
        decode_seed_line(&at_bound, FrameDirection::Request),
        Err(FrameCodecError::WrongDirection)
    ));
    assert!(matches!(
        encode_seed_line(&response, FrameDirection::Request),
        Err(FrameCodecError::WrongDirection)
    ));
    assert!(matches!(
        decode_state_line(
            &encode_state_line(&open(), FrameDirection::Request).unwrap(),
            FrameDirection::Response
        ),
        Err(FrameCodecError::WrongDirection)
    ));
    let mut request = StateFrame::File {
        common: common(2),
        filing: filing(),
    };
    let overhead = encode_state_line(&request, FrameDirection::Request)
        .unwrap()
        .len();
    if let StateFrame::File { filing, .. } = &mut request {
        filing
            .partition_rule_id
            .push_str(&"a".repeat(STATE_REQUEST_LINE_BOUND - overhead));
    }
    assert_eq!(
        encode_state_line(&request, FrameDirection::Request)
            .unwrap()
            .len(),
        STATE_REQUEST_LINE_BOUND
    );
    if let StateFrame::File { filing, .. } = &mut request {
        filing.partition_rule_id.push('a');
    }
    assert!(matches!(
        encode_state_line(&request, FrameDirection::Request),
        Err(FrameCodecError::FrameTooLarge)
    ));
}

#[test]
fn encoder_refuses_multiline_raw_values_and_zero_exchange_ids() {
    let raw = "{\n\"type\":\"text\",\"text\":\"Read the file.\"}";
    let mut record = filing();
    record.raw_block = RawValue::from_string(raw.into()).unwrap();
    let frame = StateFrame::File {
        common: common(2),
        filing: record,
    };
    assert!(matches!(
        encode_state_line(&frame, FrameDirection::Request),
        Err(FrameCodecError::EmbeddedNewline)
    ));
    let mut seed = seed_request();
    seed.raw_block = RawValue::from_string(raw.into()).unwrap();
    assert!(matches!(
        encode_seed_line(
            &SeedFrame::Seed {
                request: Box::new(seed)
            },
            FrameDirection::Request
        ),
        Err(FrameCodecError::EmbeddedNewline)
    ));
    let bad = StateFrame::File {
        common: common(0),
        filing: filing(),
    };
    assert!(encode_state_line(&bad, FrameDirection::Request).is_err());
    let mut duplicate = filing();
    duplicate.raw_block =
        RawValue::from_string(r#"{"type":"text","text":"Read the file.","text":"changed"}"#.into())
            .unwrap();
    assert!(
        encode_state_line(
            &StateFrame::File {
                common: common(2),
                filing: duplicate,
            },
            FrameDirection::Request
        )
        .is_err()
    );
}
