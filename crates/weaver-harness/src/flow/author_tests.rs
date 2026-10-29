use super::*;

fn authored(kind: Kind, source: AuthoredSource) -> AuthoredEvent {
    AuthoredEvent {
        kind,
        subsystem: Subsystem::Harness,
        turn: None,
        source,
        causal_parent: Some(Sequence(17)),
        payload: None,
    }
}

#[test]
fn boundary_identity_not_payload_elects_input_origin() {
    let author = ServingAuthor::new("s".into(), "r".into(), 1000, 100).unwrap();
    for (uid, expected) in [(1000, Origin::Operator), (1001, Origin::External)] {
        let event = author
            .prepare(
                &authored(Kind::MessageUser, AuthoredSource::BoundaryInput(uid)),
                130,
                1,
            )
            .unwrap();
        assert_eq!(
            (event.envelope.channel, event.envelope.origin),
            (Channel::Input, expected)
        );
        assert_eq!(event.envelope.causal_parent, Some(Sequence(17)));
        assert_eq!(event.envelope.monotonic_ns, MonotonicNs(30));
    }
    assert_eq!(
        author
            .prepare(&authored(Kind::MessageUser, AuthoredSource::Loop), 130, 1)
            .unwrap_err(),
        AuthorError::UnlicensedSource
    );
}

#[test]
fn all_registered_kinds_have_only_their_declared_sources() {
    use AuthoredSource as S;
    let source_values = [
        S::BoundaryInput(1000),
        S::ManagementInput(crate::flow::plan::ManagementInput::for_test_root()),
        S::Model,
        S::Tool,
        S::Loop,
        S::Runtime,
        S::ManagementInput(crate::flow::plan::ManagementInput::for_test_uid(1000)),
    ];
    let author = ServingAuthor::new("s".into(), "r".into(), 1000, 0).unwrap();
    // This independent table is the complete HARNESS-AUTHOR census. Test the
    // forbidden source alternatives too, rather than only the permitted path.
    let rows = [
        ("input.offered message.user", vec![0]),
        ("plan.event", vec![1, 4]),
        (
            "message.assistant model.output model.unrecovered_call",
            vec![2],
        ),
        ("message.tool_result tool.return_rejected", vec![3]),
        ("tool.call.started", vec![2, 4]),
        ("tool.call.completed", vec![3, 4]),
        (
            "plan.break state.plan.measurement loop.instruction sort.decision state.sort.measurement step.outcome state.mark state.mark.position state.placement model.request",
            vec![4],
        ),
        (
            "model.measurement model.field score state.flow state.store.measurement",
            vec![5],
        ),
        (
            "classify.request classify.output ste100.result load unload session.closed turn.started turn.closed fault elision refusal flush recall save_point message.restored",
            vec![5],
        ),
        ("message.system", vec![]),
    ];
    let mut seen = std::collections::BTreeSet::new();
    for (names, allowed) in rows {
        for name in names.split_whitespace() {
            let kind = Kind::from_name(name).unwrap();
            assert!(seen.insert(kind));
            for (index, source) in source_values.iter().enumerate() {
                let result = author.prepare(&authored(kind, source.clone()), 1, 1);
                assert_eq!(
                    result.is_ok(),
                    allowed.contains(&index),
                    "{name} {source:?}"
                );
                if kind == Kind::PlanEvent && !allowed.contains(&index) {
                    assert_eq!(result.unwrap_err(), AuthorError::UnlicensedSource);
                }
            }
        }
    }
    assert_eq!(seen, Kind::ALL.iter().copied().collect());
}

#[test]
fn shared_origin_rejects_backward_clock_and_preserves_author_members() {
    let author = ServingAuthor::new("session".into(), "run".into(), 0, 10).unwrap();
    let mut request = authored(Kind::MessageAssistant, AuthoredSource::Model);
    request.turn = Some(TurnRef("turn".into()));
    let raw = "{ \"role\":\"assistant\", \"content\":[{\"type\":\"text\",\"text\":\"Read caf\\u00e9.\"}] }";
    request.payload = Some(Payload::Message(
        serde_json::value::RawValue::from_string(raw.into()).unwrap(),
    ));
    assert_eq!(
        author.prepare(&request, 9, 1).unwrap_err(),
        AuthorError::ClockBeforeOrigin
    );
    let result = author.prepare(&request, 17, 22).unwrap();
    assert_eq!(result.envelope.session.0, "session");
    assert_eq!(result.envelope.run.0, "run");
    assert_eq!(result.envelope.turn, request.turn);
    assert_eq!(result.envelope.subsystem, request.subsystem);
    assert_eq!(result.envelope.causal_parent, request.causal_parent);
    assert_eq!(result.envelope.wall_ms, 22);
    assert_eq!(result.envelope.monotonic_ns, MonotonicNs(7));
    assert_eq!(result.envelope.sequence, Sequence(0));
    assert_eq!(result.envelope.kind, request.kind);
    let Some(Payload::Message(prepared)) = &result.payload else {
        panic!("authoring discarded the supplied content payload");
    };
    assert_eq!(
        prepared.get(),
        raw,
        "authoring cannot normalize payload bytes"
    );
    let Some(Payload::Message(original)) = &request.payload else {
        panic!()
    };
    assert_eq!(
        original.get(),
        raw,
        "preparation leaves its source untouched"
    );
}
