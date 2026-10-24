use super::super::payloads::tests::payload_fixtures;
use super::*;

pub(crate) fn event(
    kind: Kind,
    channel: Channel,
    origin: Origin,
    turn: bool,
    payload: Option<Payload>,
) -> Event {
    Event {
        envelope: Envelope {
            session: SessionRef("session".into()),
            run: RunRef("run".into()),
            turn: turn.then(|| TurnRef("turn".into())),
            sequence: Sequence(999),
            kind,
            channel,
            origin,
            subsystem: Subsystem::Harness,
            causal_parent: (kind == Kind::StatePlacement).then_some(Sequence(9)),
            wall_ms: 42,
            monotonic_ns: MonotonicNs(9007199254740993),
        },
        payload,
    }
}
fn accepts(event: &Event) -> bool {
    admit(event, &SessionRef("session".into()), &RunRef("run".into())).is_ok()
}
fn fixture(kind: Kind) -> Event {
    let (kind, channel, origin, turn, payload) = payload_fixtures()
        .into_iter()
        .find(|entry| entry.0 == kind)
        .unwrap();
    event(kind, channel, origin, turn, Some(payload))
}
fn raw(text: &str) -> Box<RawValue> {
    RawValue::from_string(text.into()).unwrap()
}

#[test]
fn every_new_payload_has_one_licensed_kind_and_declared_turn_rule() {
    for (kind, channel, origin, turn, payload) in payload_fixtures() {
        let good = event(kind, channel, origin, turn, Some(payload));
        assert!(accepts(&good), "{kind:?}");
        for other in Kind::ALL.iter().copied().filter(|other| *other != kind) {
            let mut wrong = good.clone();
            wrong.envelope.kind = other;
            assert!(!accepts(&wrong), "{kind:?} payload licensed as {other:?}");
        }
        for (other_channel, other_origin) in [
            (Channel::Input, Origin::Runtime),
            (Channel::Measurement, Origin::Operator),
            (Channel::Tool, Origin::External),
        ] {
            let mut wrong = good.clone();
            wrong.envelope.channel = other_channel;
            wrong.envelope.origin = other_origin;
            assert!(!accepts(&wrong), "{kind:?} accepts unlicensed source");
        }
        let mut wrong = good.clone();
        wrong.envelope.session = SessionRef("different".into());
        assert!(!accepts(&wrong));
        wrong = good.clone();
        wrong.envelope.run = RunRef("different".into());
        assert!(!accepts(&wrong));
        wrong = good.clone();
        wrong.payload = None;
        assert!(!accepts(&wrong));
        match turn_rule(kind) {
            TurnRule::Required => {
                wrong = good;
                wrong.envelope.turn = None;
                assert!(!accepts(&wrong), "{kind:?}");
            }
            TurnRule::Forbidden => {
                wrong = good;
                wrong.envelope.turn = Some(TurnRef("turn".into()));
                assert!(!accepts(&wrong), "{kind:?}");
            }
            _ => {}
        }
    }
}

#[test]
fn brackets_omit_payload_and_ordinary_message_bytes_remain_opaque() {
    let started = event(
        Kind::TurnStarted,
        Channel::Internal,
        Origin::Runtime,
        true,
        None,
    );
    let closed = event(
        Kind::SessionClosed,
        Channel::Internal,
        Origin::Runtime,
        false,
        None,
    );
    assert!(accepts(&started));
    assert!(accepts(&closed));
    let mut wrong = closed;
    wrong.payload = Some(Payload::Fault(raw("{}")));
    assert!(!accepts(&wrong));
    for (kind, channel, origin, turn) in [
        (
            Kind::MessageSystem,
            Channel::Internal,
            Origin::Runtime,
            false,
        ),
        (Kind::MessageUser, Channel::Input, Origin::Operator, true),
        (Kind::MessageAssistant, Channel::Output, Origin::Model, true),
        (Kind::MessageToolResult, Channel::Tool, Origin::Tool, true),
        (
            Kind::MessageRestored,
            Channel::Internal,
            Origin::Runtime,
            false,
        ),
    ] {
        let mut message = event(
            kind,
            channel,
            origin,
            turn,
            Some(Payload::Message(raw(r#"{"opaque_future_message":true}"#))),
        );
        assert!(accepts(&message));
        message.envelope.subsystem = Subsystem::Tool;
        assert!(!accepts(&message));
    }
}

#[test]
fn loop_metadata_and_message_shape_are_checked_before_authoring() {
    let mut record = fixture(Kind::LoopInstruction);
    record.envelope.turn = None;
    assert!(!accepts(&record));
    let Some(Payload::LoopInstruction(payload)) = &mut record.payload else {
        panic!()
    };
    payload.instruction = LoopInstructionMeta::WindDown {};
    assert!(accepts(&record));
    record.envelope.turn = Some(TurnRef("turn".into()));
    assert!(accepts(&record));
    for malformed in [
        r#"{"role":"system","content":[{"type":"text","text":"Read file."}]}"#,
        r#"{"role":"user","content":[]}"#,
        r#"{"role":"user","content":[{"type":"text","text":"Read file."},{"type":"text","text":"Read file."}]}"#,
        r#"{"role":"user","content":[{"type":"tool_use","id":"call"}]}"#,
        r#"{"role":"user","content":[{"type":"text","text":"Read file.","extra":true}]}"#,
        r#"{"role":"user","role":"user","content":[{"type":"text","text":"Read file."}]}"#,
    ] {
        let Some(Payload::LoopInstruction(payload)) = &mut record.payload else {
            panic!()
        };
        payload.message = raw(malformed);
        assert!(!accepts(&record), "{malformed}");
    }
}

#[test]
fn pair_lane_and_abandoned_turn_licensing_are_not_inferred() {
    let mut started = fixture(Kind::ToolCallStarted);
    started.envelope.origin = Origin::Loop;
    assert!(!accepts(&started));
    started.envelope.origin = Origin::Model;
    let Some(Payload::PairStarted(payload)) = &mut started.payload else {
        panic!()
    };
    payload.lane = PairLane::External;
    assert!(!accepts(&started));
    let mut ended = fixture(Kind::ToolCallCompleted);
    ended.envelope.turn = None;
    assert!(!accepts(&ended));
    let Some(Payload::PairEnded(payload)) = &mut ended.payload else {
        panic!()
    };
    payload.reason = Some(PairFailure::AbandonedAtLoad {});
    assert!(!accepts(&ended));
    ended.envelope.origin = Origin::Loop;
    assert!(accepts(&ended));
    ended.envelope.turn = Some(TurnRef("turn".into()));
    assert!(!accepts(&ended));
}

#[test]
fn plan_and_outcome_discriminants_cannot_substitute_for_one_another() {
    for mut plan in payload_fixtures()
        .into_iter()
        .filter(|item| item.0 == Kind::PlanEvent)
        .map(|(k, c, o, t, p)| event(k, c, o, t, Some(p)))
    {
        plan.envelope.turn = Some(TurnRef("turn".into()));
        assert!(!accepts(&plan));
    }
    let mut outcome = fixture(Kind::StepOutcome);
    let Some(Payload::StepOutcome(payload)) = &mut outcome.payload else {
        panic!()
    };
    payload.outcome = ReviewOutcome::Closed {};
    assert!(!accepts(&outcome));
    let Some(Payload::StepOutcome(payload)) = &mut outcome.payload else {
        panic!()
    };
    payload.context = Some(PlanEffect::Close {
        expected: payload.plan.clone(),
        step_id: payload.step_id.clone(),
        review_call: payload.review_call.clone(),
        work: payload.work.clone(),
        outcome_span: payload.work.span.clone(),
    });
    assert!(accepts(&outcome));
    let Some(Payload::StepOutcome(payload)) = &outcome.payload else {
        panic!()
    };
    let mut close_as_plan = event(
        Kind::PlanEvent,
        Channel::Internal,
        Origin::Loop,
        true,
        Some(Payload::PlanEffect(Box::new(PlanEffectRecord {
            message: payload.message.clone(),
            context: payload.context.clone().unwrap(),
            template_id: payload.template_id.clone(),
        }))),
    );
    assert!(!accepts(&close_as_plan), "closure belongs to step.outcome");
    close_as_plan.envelope.turn = None;
    assert!(
        !accepts(&close_as_plan),
        "a turnless plan.event cannot carry closure either"
    );
    let Some(Payload::StepOutcome(payload)) = &mut outcome.payload else {
        panic!()
    };
    payload.step_id.local_id = 99.into();
    assert!(!accepts(&outcome));
}

#[test]
fn conditional_measurement_and_persistence_members_have_one_shape() {
    let mut result = fixture(Kind::Ste100Result);
    result.envelope.turn = Some(TurnRef("turn".into()));
    assert!(!accepts(&result));
    let mut flow = fixture(Kind::StateFlow);
    let Some(Payload::FlowObservation(payload)) = &mut flow.payload else {
        panic!()
    };
    payload.phase = FlowPhase::RepairRequested;
    assert!(!accepts(&flow));
    let Some(Payload::FlowObservation(payload)) = &mut flow.payload else {
        panic!()
    };
    payload.failed_corrections = Some(0.into());
    payload.retry_cap = Some(2.into());
    payload.source_id = Some(SourceBlockId {
        run: "run".into(),
        sequence: 1.into(),
        block: 0.into(),
    });
    assert!(accepts(&flow));
    let Some(Payload::FlowObservation(payload)) = &mut flow.payload else {
        panic!()
    };
    payload.source_id = None;
    assert!(!accepts(&flow));
    let mut save = fixture(Kind::SavePoint);
    for invalid in [PersistenceCause::Seed, PersistenceCause::Rebind] {
        let Some(Payload::SavePointRecord(payload)) = &mut save.payload else {
            panic!()
        };
        let SavePointRecord::Requested { cause, .. } = payload.as_mut() else {
            panic!()
        };
        *cause = invalid;
        assert!(!accepts(&save));
    }
    let mut unload = fixture(Kind::Unload);
    let Some(Payload::Unload(payload)) = &mut unload.payload else {
        panic!()
    };
    payload.end_reason = Some(EndReason::ComponentFailure);
    assert!(!accepts(&unload));
    let Some(Payload::Unload(payload)) = &mut unload.payload else {
        panic!()
    };
    payload.state_flow_failure = Some(StateFlowFailure {
        fault_sequence: None,
        account: raw("{}"),
    });
    assert!(accepts(&unload));
    let mut placement = fixture(Kind::StatePlacement);
    let Some(Payload::Placement(payload)) = &mut placement.payload else {
        panic!()
    };
    payload.assembly_id = 1.into();
    assert!(!accepts(&placement));
}

#[test]
fn boot_placement_refuses_a_cut_reference_but_cut_preparation_may_omit_it() {
    let mut placement = fixture(Kind::StatePlacement);
    assert!(accepts(&placement));
    let Some(Payload::Placement(payload)) = &mut placement.payload else {
        panic!()
    };
    payload.cut = Some(CutReference {
        span: SpanRef {
            seed_id: "seed".into(),
            local_id: 1.into(),
            revision: 1.into(),
        },
        occurrence_id: MarkOccurrenceId {
            run: "run".into(),
            ordinal: 1.into(),
        },
    });
    assert!(matches!(
        admit(
            &placement,
            &SessionRef("session".into()),
            &RunRef("run".into())
        ),
        Err(Failure::RefusedOnSubmit {
            reason: SubmitRefusal::PayloadMalformed,
        })
    ));

    let Some(Payload::Placement(payload)) = &mut placement.payload else {
        panic!()
    };
    payload.phase = PlacementPhase::Cut;
    payload.assembly_id = 1.into();
    payload.method = Some(CutMethod::Flush);
    payload.open_evidence = None;
    assert!(accepts(&placement));
    let Some(Payload::Placement(payload)) = &mut placement.payload else {
        panic!()
    };
    payload.cut = None;
    assert!(accepts(&placement));
}

#[test]
fn boot_placement_requires_its_turnless_outer_facts_without_electing_fit() {
    let good = fixture(Kind::StatePlacement);
    assert!(accepts(&good));
    let mut bad = good.clone();
    bad.envelope.turn = Some(TurnRef("turn".into()));
    assert!(!accepts(&bad));
    let mut bad = good.clone();
    bad.envelope.causal_parent = None;
    assert!(!accepts(&bad));

    type Change = (&'static str, fn(&mut PlacementRecord));
    let changes: &[Change] = &[
        ("assembly", |p| p.assembly_id = 1.into()),
        ("context", |p| {
            p.context = Some(PlacementContext {
                label: "invented".into(),
                instruction: None,
            });
        }),
        ("method", |p| p.method = Some(CutMethod::Flush)),
        ("N", |p| p.n += 1),
        ("resident_before", |p| p.resident_before += 1),
        ("input_tokens", |p| p.input_tokens = 1),
        ("capacity", |p| p.capacity = p.prefix_tokens - 1),
        ("terminal_tokens", |p| p.terminal_tokens = 0),
        ("unit_map", |p| {
            p.unit_map.push(UnitMapEntry {
                unit_id: UnitId::Pair {
                    call_id: CallId {
                        run: "run".into(),
                        sequence: 1.into(),
                        call: 0.into(),
                    },
                },
                occurrence_id: UnitOccurrenceId {
                    run: "run".into(),
                    assembly_id: 0.into(),
                    unit_index: 0.into(),
                },
                message_start: 0.into(),
                message_end: 1.into(),
                spans: vec![],
                source_map: vec![],
                step_queries: vec![],
            });
        }),
        ("message_ids", |p| p.message_ids.push(1.into())),
        ("assistant_message_id", |p| {
            p.assistant_message_id = Some(1.into());
        }),
        ("recomputed_messages", |p| p.recomputed_messages = 1.into()),
        ("open_evidence", |p| p.open_evidence = None),
    ];
    for (name, change) in changes {
        let mut bad = good.clone();
        let Some(Payload::Placement(record)) = &mut bad.payload else {
            unreachable!()
        };
        change(record);
        assert!(!accepts(&bad), "{name}");
    }

    let mut seated = good;
    let Some(Payload::Placement(record)) = &mut seated.payload else {
        unreachable!()
    };
    // Empty valid prefixes and unsuccessful configured fit still have a
    // truthful boot account. Trace never supplies journal/readiness authority.
    record.n = 0;
    record.resident_before = 0;
    record.prefix_tokens = 0;
    record.capacity = 0;
    record.generation_limit = u64::MAX;
    assert!(accepts(&seated));
}

#[test]
fn open_evidence_checks_object_boxes_without_decoding_spu_members() {
    for value in ["null", "[]", "0", "true", r#""object""#] {
        for request in [false, true] {
            let mut placement = fixture(Kind::StatePlacement);
            let Some(Payload::Placement(record)) = &mut placement.payload else {
                unreachable!()
            };
            let evidence = record.open_evidence.as_mut().unwrap();
            if request {
                evidence.request = raw(value);
            } else {
                evidence.measurement = raw(value);
            }
            assert!(!accepts(&placement), "request={request}: {value}");
        }
    }
    for value in [
        "{}",
        r#"{ "future": {"v":1}, "future": {"v":2} }"#,
        r#"{"prefix_token_ids":[-1],"blocks":[{"start":"invalid"}]}"#,
    ] {
        let mut placement = fixture(Kind::StatePlacement);
        let Some(Payload::Placement(record)) = &mut placement.payload else {
            unreachable!()
        };
        record.open_evidence = Some(OpenEvidenceRecord {
            request: raw(value),
            measurement: raw(value),
        });
        assert!(accepts(&placement), "trace must leave SPU contents opaque");
    }
}

#[test]
fn shared_open_corpus_keeps_harness_semantics_out_of_trace_admission() {
    use super::super::record::Object;

    let cases: serde_json::Value = serde_json::from_str(include_str!(
        "../../../weaver-types/tests/fixtures/decode-open-evidence.json"
    ))
    .unwrap();
    let mut opaque_cases = 0;
    for case in cases.as_array().unwrap() {
        if !case["outer_valid"].as_bool().unwrap() {
            continue;
        }
        let wire = Object::parse(case["wire"].as_str().unwrap()).unwrap();
        let body = Object::parse(wire.get("body").unwrap().get()).unwrap();
        let number = |field| serde_json::from_str::<u64>(body.get(field).unwrap().get()).unwrap();
        let request = body.get("request").unwrap().get();
        let measurement = body.get("measurement").unwrap().get();
        let mut placement = fixture(Kind::StatePlacement);
        let Some(Payload::Placement(record)) = &mut placement.payload else {
            unreachable!()
        };
        record.prefix_tokens = number("prefix_tokens");
        record.n = record.prefix_tokens;
        record.resident_before = record.prefix_tokens;
        record.capacity = number("capacity");
        record.generation_limit = number("generation_limit");
        record.terminal_tokens = number("terminal_tokens");
        record.open_evidence = Some(OpenEvidenceRecord {
            request: raw(request),
            measurement: raw(measurement),
        });
        // Trace sees only its local count predicates and object-box shape.
        // The harness owns the wire session and every nested evidence member.
        let expected = record.prefix_tokens <= record.capacity
            && record.terminal_tokens == 1
            && serde_json::from_str::<serde_json::Value>(request)
                .unwrap()
                .is_object()
            && serde_json::from_str::<serde_json::Value>(measurement)
                .unwrap()
                .is_object();
        let encoded = serde_json::to_string(record).unwrap();
        assert!(encoded.contains(&format!(
            "\"open_evidence\":{{\"request\":{request},\"measurement\":{measurement}}}"
        )));
        assert_eq!(accepts(&placement), expected, "{}", case["name"]);
        if expected && !case["evidence_valid"].as_bool().unwrap() {
            opaque_cases += 1;
        }
    }
    assert!(
        opaque_cases > 0,
        "corpus must exercise opaque invalid contents"
    );
}

#[test]
fn nonboot_placement_requires_null_open_evidence_and_nonzero_assembly() {
    for phase in [PlacementPhase::Cut, PlacementPhase::Append] {
        let mut placement = fixture(Kind::StatePlacement);
        let Some(Payload::Placement(record)) = &mut placement.payload else {
            unreachable!()
        };
        record.phase = phase;
        record.assembly_id = 1.into();
        record.method = Some(CutMethod::Flush);
        record.context = (phase == PlacementPhase::Append).then(|| PlacementContext {
            label: "identity".into(),
            instruction: None,
        });
        assert!(!accepts(&placement));
        let Some(Payload::Placement(record)) = &mut placement.payload else {
            unreachable!()
        };
        record.open_evidence = None;
        assert!(
            serde_json::to_string(record)
                .unwrap()
                .ends_with(",\"open_evidence\":null}")
        );
        assert!(accepts(&placement));
        let Some(Payload::Placement(record)) = &mut placement.payload else {
            unreachable!()
        };
        record.assembly_id = 0.into();
        assert!(!accepts(&placement));
    }
}
