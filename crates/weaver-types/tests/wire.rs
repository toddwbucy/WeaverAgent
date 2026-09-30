//! conforms: types-loop0-encoding-json
//! conforms: types-tagging-test
//! conforms: types-frame-survives-arbitrary-octets
//!
//! The wire-shape tests of `weaver-types-Spec` section 4.3. The Spec states the
//! envelope's layout rather than leaving it to a reader, and these tests hold
//! the code to the stated octets, including the two failure shapes an earlier
//! draft of the Spec verified against serde 1.x: duplicate tag names and
//! newtype variants that cannot serialize.

use weaver_types::{
    AgentName, AgentState, AgentSummary, ExchangeId, LifecycleAnswer, LifecycleDirective,
    LifecycleRefusal, Opener, OrganEnvelope, Payload, Position, RefusingOrgan,
};

/// The envelope's layout, exactly as the Spec states it.
#[test]
fn envelope_layout_is_the_stated_shape() {
    let envelope = OrganEnvelope {
        exchange: ExchangeId {
            opener: Opener::Admin,
            ordinal: 7,
        },
        position: Position::Open,
        payload: Payload::Directive(LifecycleDirective::Load {
            agent: AgentName("alpha".to_string()),
        }),
    };
    let json = serde_json::to_string(&envelope).expect("serializes");
    assert_eq!(
        json,
        concat!(
            "{\"exchange\":{\"opener\":\"admin\",\"ordinal\":7},",
            "\"position\":\"open\",",
            "\"payload\":{\"kind\":\"directive\",\"body\":{\"kind\":\"load\",\"agent\":\"alpha\"}}}"
        )
    );
    let back: OrganEnvelope = serde_json::from_str(&json).expect("deserializes");
    assert_eq!(envelope, back);
}

/// The sequence-carrying case round-trips, which is what the adjacent tagging
/// of `Payload` exists to make possible.
#[test]
fn sequence_carrying_answer_round_trips() {
    let envelope = OrganEnvelope {
        exchange: ExchangeId {
            opener: Opener::Admin,
            ordinal: 9,
        },
        position: Position::Close,
        payload: Payload::Answer(LifecycleAnswer::Agents {
            agents: vec![
                AgentSummary {
                    name: AgentName("alpha".into()),
                    state: AgentState::Idle,
                    load: None,
                },
                AgentSummary {
                    name: AgentName("beta".into()),
                    state: AgentState::Unloaded,
                    load: None,
                },
            ],
        }),
    };
    let json = serde_json::to_string(&envelope).expect("serializes");
    let back: OrganEnvelope = serde_json::from_str(&json).expect("deserializes");
    assert_eq!(envelope, back);
}

/// The boxed aggregate case round-trips: a refusing organ's reason reaches
/// admin without translation.
#[test]
fn organ_refused_carries_the_inner_reason_unchanged() {
    let refusal = LifecycleRefusal::OrganRefused {
        organ: RefusingOrgan::Spu,
        reason: Box::new(LifecycleRefusal::DeviceCannotAdmit),
    };
    let json = serde_json::to_string(&refusal).expect("serializes");
    assert_eq!(
        json,
        "{\"kind\":\"organ_refused\",\"organ\":\"spu\",\"reason\":{\"kind\":\"device_cannot_admit\"}}"
    );
    let back: LifecycleRefusal = serde_json::from_str(&json).expect("deserializes");
    assert_eq!(refusal, back);
}

/// **The two boxed payloads cross as the payloads do**, per
/// `weaver-types-Spec` section 4.2 as of 2026-09-06 and issue #475: `Enter`
/// carries its payload boxed and `State` its load boxed, and the box is not
/// on the wire. Each value is serialized, read back, and compared whole, and
/// the state's rendering is pinned to the shape a reader of `main` met, so
/// a change to the wire's spelling fails here rather than at a peer.
#[test]
fn the_boxed_payloads_cross_as_the_payloads_do() {
    let enter = LifecycleDirective::Enter {
        payload: Box::new(weaver_types::EnterPayload {
            session: weaver_types::SessionId("s-1".into()),
            run: weaver_types::RunId("r-1".into()),
            spu_instruction: weaver_types::SpuInstruction {
                classify: None,
                decoder: weaver_types::DecoderInstruction {
                    model_binding: weaver_types::ModelBinding {
                        artifact: weaver_types::ArtifactRef("/models/a.gguf".into()),
                        devices: vec![weaver_types::DeviceOrdinal(0)],
                    },
                    residual_readout_election: false,
                    field_election: None,
                    surprisal_election: false,
                    refeed_permission: false,
                    column_permission: false,
                    identity: Vec::new(),
                    tunable_values: Default::default(),
                },
            },
            binding: weaver_types::EnterBinding::Serving {
                gate_instruction: weaver_types::GateInstruction {
                    access_rule: weaver_types::AccessRule {
                        allowed_uids: Default::default(),
                        allowed_gids: Default::default(),
                        denied_uids: Default::default(),
                    },
                },
            },
            state_store: weaver_types::StateStore::default(),
            declaration: String::new(),
            restore: None,
            stack: Default::default(),
            state_election: weaver_types::StateElection {
                all_kinds: false,
                keys: Vec::new(),
            },
        }),
    };
    let json = serde_json::to_string(&enter).expect("serializes");
    assert!(json.starts_with("{\"kind\":\"enter\",\"payload\":{\"session\":\"s-1\""));
    let back: LifecycleDirective = serde_json::from_str(&json).expect("deserializes");
    assert_eq!(enter, back);

    let state = LifecycleAnswer::State {
        state: AgentState::Idle,
        load: Some(Box::new(weaver_types::LoadFacts {
            session: weaver_types::SessionId("s-1".into()),
            run: weaver_types::RunId("r-1".into()),
            declaration: "d".into(),
            artifact: weaver_types::ArtifactRef("/models/a.gguf".into()),
            residual_readout: false,
            field: None,
            surprisal: false,
            state_election: weaver_types::StateElection {
                all_kinds: false,
                keys: Vec::new(),
            },
            state_store: weaver_types::StateStore::default(),
            state_member: false,
            composer: weaver_types::Composer {
                binary: "weaver-harness".into(),
                file: None,
                sha256: None,
            },
        })),
    };
    let json = serde_json::to_string(&state).expect("serializes");
    assert_eq!(
        json,
        concat!(
            "{\"kind\":\"state\",\"state\":\"idle\",\"load\":{",
            "\"session\":\"s-1\",\"run\":\"r-1\",\"declaration\":\"d\",",
            "\"artifact\":\"/models/a.gguf\",\"residual_readout\":false,",
            "\"surprisal\":false,\"state_election\":{\"all-kinds\":false,\"keys\":[]},",
            "\"state_store\":{\"engine\":\"sqlite\",\"database\":null,\"role\":null},",
            "\"state_member\":false,\"composer\":{\"binary\":\"weaver-harness\"}}}"
        ),
        "the load crosses unboxed and in the Spec's spelling"
    );
    let back: LifecycleAnswer = serde_json::from_str(&json).expect("deserializes");
    assert_eq!(state, back);
}

/// An unknown payload kind refuses rather than defaulting, the same refusal
/// posture the message model holds.
#[test]
fn unknown_payload_kind_refuses() {
    let json = "{\"kind\":\"telemetry\",\"body\":{}}";
    assert!(serde_json::from_str::<Payload>(json).is_err());
}

/// **The measurement splices and never quotes**, per `weaver-types-Spec`
/// section 4.4: the payload of `Generated` carries pre-serialized JSON as a
/// member of the answer's object, and a quoted string in its place is the
/// double encoding the `RawValue` election exists to prevent.
///
/// The round trip is through bytes, because the read side is where a tagging
/// that buffers fails: a shape that serializes clean and cannot come back is
/// a wire type only one party can use.
#[test]
fn the_measurement_splices_and_never_quotes() {
    let measurement = serde_json::value::RawValue::from_string(
        r#"{"tokens":[151643,872],"timings":{"prefill_ms":12}}"#.to_string(),
    )
    .expect("valid JSON splices");
    let request = serde_json::value::RawValue::from_string(
        serde_json::json!({"rendered":"<|im_start|>user\nhi<|im_end|>\n","template":"qwen2","sampling":{}}).to_string(),
    )
    .expect("valid JSON splices");
    let answer = weaver_types::TokenAnswer::Generated(weaver_types::Generation {
        content: vec![weaver_traits::ContentBlock::ToolCall(
            weaver_traits::ToolCall {
                name: "calculator".to_string(),
                arguments: r#"{"expression":"1+1"}"#.to_string(),
            },
        )],
        emission: "two words".to_string(),
        finish: weaver_types::Finish::Completed,
        request,
        measurement,
        resident: 64,
        capacity: 4096,
    });

    let bytes = serde_json::to_string(&answer).expect("serializes");
    let encoded: serde_json::Value = serde_json::from_str(&bytes).expect("valid JSON");
    assert_eq!(
        encoded["kind"], "generated",
        "adjacently tagged under the spliced-member arm, not externally: {bytes}"
    );
    assert!(
        encoded["body"]["measurement"].is_object(),
        "the measurement nests in the adjacent body as an object: {bytes}"
    );
    assert_eq!(
        encoded["body"]["resident"], 64,
        "the fullness rides as typed members of the body: {bytes}"
    );
    assert_eq!(encoded["body"]["capacity"], 4096, "{bytes}");
    assert!(
        bytes.contains(r#""measurement":{"tokens":[151643,872]"#),
        "the measurement is a member, spliced: {bytes}"
    );
    assert!(
        !bytes.contains(r#""measurement":"{"#),
        "and never a quoted string: {bytes}"
    );

    let back: weaver_types::TokenAnswer = serde_json::from_str(&bytes).expect("deserializes");
    assert_eq!(back, answer, "and the round trip holds through bytes");
}

/// The directive's ask round-trips, and carries no sampling value.
///
/// **The absence is asserted rather than assumed.** The tunable map left this
/// directive when the values moved to the declaration, per `weaver-spu-Spec`
/// section 8, and a serialization that grew one back would be the seam
/// carrying inbound what the decode contract's conformance list says it does
/// not.
#[test]
fn the_token_directive_round_trips_through_bytes() {
    let directive = weaver_types::TokenDirective::AppendAndGenerate {
        turn: weaver_types::TurnKey("t-7".into()),
        delta: vec![],
    };
    let bytes = serde_json::to_string(&directive).expect("serializes");
    assert!(
        bytes.contains(r#""kind":"append_and_generate""#),
        "internally tagged, snake case: {bytes}"
    );
    assert!(
        !bytes.contains("tunable") && !bytes.contains("temperature"),
        "no sampling value crosses this seam inbound: {bytes}"
    );
    let back: weaver_types::TokenDirective = serde_json::from_str(&bytes).expect("deserializes");
    assert_eq!(back, directive);
}

/// The refusal's overflow carries the session's own account, per the decode
/// contract's section 5, and the fieldless cases are plain tagged strings.
#[test]
fn the_token_refusal_carries_the_account() {
    let refusal = weaver_types::TokenRefusal::Overflow {
        resident: 4096,
        requested: 512,
        capacity: 4352,
    };
    let bytes = serde_json::to_string(&refusal).expect("serializes");
    assert!(bytes.contains(r#""kind":"overflow""#) && bytes.contains(r#""resident":4096"#));
    let back: weaver_types::TokenRefusal = serde_json::from_str(&bytes).expect("deserializes");
    assert_eq!(back, refusal);
    let not_open: weaver_types::TokenRefusal =
        serde_json::from_str(r#"{"kind":"not_open"}"#).expect("a fieldless case reads");
    assert_eq!(not_open, weaver_types::TokenRefusal::NotOpen);
}

/// Arbitrary octets round-trip the frame and the envelope, per the frame
/// election of `weaver-types-Spec` section 4.1: every byte value, no UTF-8
/// assumed, the encoding surviving what a splice would not.
#[test]
fn the_frame_survives_arbitrary_octets() {
    let octets: Vec<u8> = (0u8..=255).cycle().take(700).collect();
    let envelope = OrganEnvelope {
        exchange: ExchangeId {
            opener: Opener::Gate,
            ordinal: 3,
        },
        position: Position::Open,
        payload: Payload::Frame(weaver_types::TurnFrame::carry(&octets)),
    };
    let json = serde_json::to_string(&envelope).expect("serializes");
    let back: OrganEnvelope = serde_json::from_str(&json).expect("deserializes");
    let Payload::Frame(frame) = back.payload else {
        panic!("the frame came back as something else");
    };
    assert_eq!(frame.octets().expect("canonical"), octets);
}

/// Empty octets are a legal frame: the empty member, decoding to nothing.
#[test]
fn the_empty_frame_carries_and_returns_nothing() {
    let frame = weaver_types::TurnFrame::carry(b"");
    assert_eq!(frame.octets, "");
    assert_eq!(frame.octets().expect("canonical"), Vec::<u8>::new());
}

/// The decode refuses what the encode would not produce, per the election:
/// one octet sequence, exactly one carried form. Each rejected member is a
/// second spelling, off-boundary, whitespace-bearing, interior-padded, or
/// carrying trailing bits the encode zeroes.
#[test]
fn the_decode_refuses_the_noncanonical_forms() {
    for bad in [
        "QQ",       // off the four-boundary
        "QQ=",      // off the four-boundary with padding
        " QQ==",    // leading whitespace
        "Q Q==",    // interior whitespace
        "Q\nQ==",   // a line break
        "QR==",     // trailing bits set where two octets are absent
        "QUJ=",     // trailing bits set where one octet is absent
        "QQ==QQ==", // padding anywhere but the tail
        "=QQ=",     // padding leading a group
    ] {
        let frame = weaver_types::TurnFrame {
            octets: bad.to_string(),
        };
        assert!(frame.octets().is_none(), "{bad:?} decoded as canonical");
    }
    let canonical = weaver_types::TurnFrame {
        octets: "QUI=".to_string(),
    };
    assert_eq!(canonical.octets().expect("canonical"), b"AB");
}

/// The label trio round-trips through bytes, per `weaver-types-Spec` section
/// 4.5: the directive and refusal internally tagged, the answer adjacent
/// because the fault's account splices, which is the shared tagging test's
/// spliced-member arm and would fail internal tagging at deserialization.
///
/// Perturbation: drop `content = "body"` from `LabelAnswer` and the answer
/// tags internally, which flattens the members the adjacent body nests and,
/// at the fault arm further down, cannot represent the spliced account on the
/// read side. Verified by removing that member and watching this test fail at
/// the `Scored` arm's adjacent-body assertion, `the labels nest in the
/// adjacent body`, which is reached before the fault arm and so before the
/// splice's return.
#[test]
fn the_label_trio_round_trips_through_bytes() {
    let ask = weaver_types::LabelDirective::Classify {
        turn: Some(weaver_types::TurnKey("t-3".into())),
        content: "the recalled passage".to_string(),
    };
    let bytes = serde_json::to_string(&ask).expect("serializes");
    assert!(bytes.contains(r#""kind":"classify""#), "{bytes}");
    let back: weaver_types::LabelDirective = serde_json::from_str(&bytes).expect("returns");
    assert_eq!(back, ask);

    let no_turn = weaver_types::LabelDirective::Classify {
        turn: None,
        content: "between turns".to_string(),
    };
    let bytes = serde_json::to_string(&no_turn).expect("serializes");
    let back: weaver_types::LabelDirective = serde_json::from_str(&bytes).expect("returns");
    assert_eq!(back, no_turn, "the turn identity is conditional: {bytes}");

    let scored = weaver_types::LabelAnswer::Scored {
        turn: Some(weaver_types::TurnKey("t-3".into())),
        labels: vec![
            weaver_types::ScoredLabel {
                label: "entailment".into(),
                score: 0.91,
            },
            weaver_types::ScoredLabel {
                label: "not_entailment".into(),
                score: 0.09,
            },
        ],
    };
    let ready = weaver_types::LabelAnswer::Ready;
    let bytes = serde_json::to_string(&ready).expect("serializes");
    let encoded: serde_json::Value = serde_json::from_str(&bytes).expect("valid JSON");
    assert_eq!(
        encoded["kind"], "ready",
        "the readiness emission carries the adjacent envelope: {bytes}"
    );
    let back: weaver_types::LabelAnswer = serde_json::from_str(&bytes).expect("returns");
    assert_eq!(back, ready);

    let bytes = serde_json::to_string(&scored).expect("serializes");
    let encoded: serde_json::Value = serde_json::from_str(&bytes).expect("valid JSON");
    assert_eq!(
        encoded["kind"], "scored",
        "the answer is adjacently tagged: {bytes}"
    );
    assert!(
        encoded["body"]["labels"].is_array() && encoded["body"]["labels"][0]["label"].is_string(),
        "the labels nest in the adjacent body: {bytes}"
    );
    let back: weaver_types::LabelAnswer = serde_json::from_str(&bytes).expect("returns");
    assert_eq!(back, scored);

    let account = serde_json::value::RawValue::from_string(
        r#"{"organ":"spu-classify","detail":"device lost"}"#.to_string(),
    )
    .expect("valid JSON splices");
    let fault = weaver_types::LabelAnswer::Fault(weaver_types::FaultReport {
        case: weaver_types::FaultCase::DeviceFaultDuringGeneration,
        account,
    });
    let bytes = serde_json::to_string(&fault).expect("serializes");
    let encoded: serde_json::Value = serde_json::from_str(&bytes).expect("valid JSON");
    assert_eq!(encoded["kind"], "fault", "{bytes}");
    assert!(
        encoded["body"]["account"].is_object(),
        "the account is a member, spliced as an object: {bytes}"
    );
    assert_eq!(
        encoded["body"]["account"]["organ"], "spu-classify",
        "{bytes}"
    );
    let back: weaver_types::LabelAnswer = serde_json::from_str(&bytes).expect("the splice returns");
    assert_eq!(back, fault);

    let refusal = weaver_types::LabelRefusal::Oversized {
        requested: 9000,
        bound: 8192,
    };
    let bytes = serde_json::to_string(&refusal).expect("serializes");
    let encoded: serde_json::Value = serde_json::from_str(&bytes).expect("valid JSON");
    assert_eq!(encoded["kind"], "oversized", "{bytes}");
    let back: weaver_types::LabelRefusal = serde_json::from_str(&bytes).expect("returns");
    assert_eq!(back, refusal);
}

/// The seam's receive distinguishes an answer from a refusal by trying the
/// two vocabularies in order, so the property it rests on is watched here:
/// no refusal's serialization deserializes as an answer.
#[test]
fn no_label_refusal_reads_as_an_answer() {
    for refusal in [
        weaver_types::LabelRefusal::NotAdmitted {
            reason: "device lost".into(),
        },
        weaver_types::LabelRefusal::NotReady,
        weaver_types::LabelRefusal::Oversized {
            requested: 9000,
            bound: 8192,
        },
        weaver_types::LabelRefusal::MalformedContent,
    ] {
        let bytes = serde_json::to_string(&refusal).expect("serializes");
        assert!(
            serde_json::from_str::<weaver_types::LabelAnswer>(&bytes).is_err(),
            "{bytes} must not read as an answer"
        );
        let back: weaver_types::LabelRefusal = serde_json::from_str(&bytes).expect("returns");
        assert_eq!(back, refusal);
    }
}
