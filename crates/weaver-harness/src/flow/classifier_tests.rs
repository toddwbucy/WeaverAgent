use super::*;
use scripted::{ScriptStep, ScriptedPort, fixture_scheme};
use weaver_types::state_flow::{
    Binding, DecimalU64, NounBinding, SourceBlockId, Span, Ste100DefectCode, Ste100Result,
    Ste100Target, TemplatePart, TemplateSpec,
};

// Type property, not a behavioral assertion. A Clone implementation adds a
// second applicable trait instance and makes this inference ambiguous.
const _: fn() = || {
    trait InputGrantMustNotClone<Marker> {
        fn check() {}
    }
    impl<T: ?Sized> InputGrantMustNotClone<()> for T {}
    impl<T: Clone> InputGrantMustNotClone<u8> for T {}
    let _ = <AcceptedReuse as InputGrantMustNotClone<_>>::check;
};

fn request(id: u64, block: u64) -> Ste100Request {
    Ste100Request {
        request_id: id.into(),
        target: Ste100Target::Source(SourceBlockId {
            run: "source run".into(),
            sequence: 3.into(),
            block: block.into(),
        }),
        text: "Read caf\u{e9}.".into(),
        scheme_id: "fixture-read/1".into(),
    }
}

fn span(start: u64, end: u64) -> Span {
    Span {
        start: start.into(),
        end: end.into(),
    }
}

fn binding() -> Binding {
    Binding {
        statement_span: span(0, 11),
        verb: "read".into(),
        verb_span: span(0, 4),
        condition: None,
        condition_span: None,
        nouns: vec![NounBinding {
            value: "caf\u{e9}".into(),
            span: span(5, 10),
        }],
    }
}

fn accepted(request: &Ste100Request) -> Ste100Reply {
    Ste100Reply {
        request_id: request.request_id,
        target: request.target.clone(),
        scheme_id: request.scheme_id.clone(),
        result: Ste100Result::Accepted {
            bindings: vec![binding()],
        },
    }
}

fn bindings(reply: &mut Ste100Reply) -> &mut Vec<Binding> {
    let Ste100Result::Accepted { bindings } = &mut reply.result else {
        panic!("accepted fixture");
    };
    bindings
}

#[test]
fn whole_result_validation_keeps_exact_targets_and_text() {
    let mut request = request(1, 0);
    request.text.push_str("\r\n");
    let original = request.clone();
    let reply = accepted(&request);
    assert_eq!(validate_reply(&reply, &request, &fixture_scheme()), Ok(()));
    assert_eq!(request, original);
    for target in [
        Ste100Target::Input {
            input_id: 4.into(),
            block: 1.into(),
        },
        Ste100Target::Source(SourceBlockId {
            run: "another run".into(),
            sequence: 5.into(),
            block: 2.into(),
        }),
    ] {
        request.target = target;
        assert_eq!(
            validate_reply(&accepted(&request), &request, &fixture_scheme()),
            Ok(())
        );
    }
}

#[test]
fn a_wrong_identity_or_any_invalid_binding_fails_the_whole_reply() {
    type Alter = fn(&mut Ste100Reply);
    let changes: &[(&str, Alter)] = &[
        ("request id", |reply| reply.request_id = 2.into()),
        ("target", |reply| {
            reply.target = Ste100Target::Input {
                input_id: 4.into(),
                block: 0.into(),
            }
        }),
        ("scheme", |reply| reply.scheme_id = "other".into()),
        ("empty bindings", |reply| bindings(reply).clear()),
        ("statement boundary", |reply| {
            bindings(reply)[0].statement_span.end = 12.into()
        }),
        ("UTF-8 boundary", |reply| {
            bindings(reply)[0].nouns[0].span.end = 9.into()
        }),
        ("noun substring", |reply| {
            bindings(reply)[0].nouns[0].value = "cafe".into()
        }),
        ("missing noun", |reply| bindings(reply)[0].nouns.clear()),
        ("verb dictionary", |reply| {
            bindings(reply)[0].verb = "write".into()
        }),
        ("verb outside statement", |reply| {
            bindings(reply)[0].statement_span = span(5, 11)
        }),
        ("condition nullability", |reply| {
            bindings(reply)[0].condition = Some("if ready".into())
        }),
        ("invalid later binding", |reply| {
            let mut later = binding();
            later.nouns.clear();
            bindings(reply).push(later);
        }),
    ];
    let request = request(1, 0);
    for (name, change) in changes {
        let mut reply = accepted(&request);
        change(&mut reply);
        let failure = validate_reply(&reply, &request, &fixture_scheme()).expect_err(name);
        assert_eq!(
            failure.kind,
            PortFailureKind::Component(Ste100ComponentCode::InvalidResult),
            "{name}"
        );
        assert_eq!(
            failure.received,
            Some(Box::new(reply)),
            "{name}: received identity is retained"
        );
    }
    let mut wrong_loaded = fixture_scheme();
    wrong_loaded.scheme_id = "another scheme".into();
    assert!(validate_reply(&accepted(&request), &request, &wrong_loaded).is_err());
}

#[test]
fn binding_statement_order_is_the_shared_source_order() {
    let mut request = request(1, 0);
    request.text = "Read caf\u{e9}. Read caf\u{e9}.".into();
    let mut later = binding();
    later.statement_span = span(12, 23);
    later.verb_span = span(12, 16);
    later.nouns[0].span = span(17, 22);
    let mut reply = accepted(&request);
    bindings(&mut reply).push(later);
    assert!(validate_reply(&reply, &request, &fixture_scheme()).is_ok());
    bindings(&mut reply).reverse();
    assert!(validate_reply(&reply, &request, &fixture_scheme()).is_err());
}

#[test]
fn rejection_and_all_terminal_codes_keep_their_distinct_results() {
    let request = request(1, 0);
    let mut reply = accepted(&request);
    reply.result = Ste100Result::NotSte100 {
        code: Ste100DefectCode::NonSte100,
        detail: "fixture language defect".into(),
        spans: vec![span(5, 10)],
    };
    assert_eq!(validate_reply(&reply, &request, &fixture_scheme()), Ok(()));
    for code in [
        Ste100ComponentCode::Crashed,
        Ste100ComponentCode::NoResult,
        Ste100ComponentCode::InvalidResult,
        Ste100ComponentCode::TransportFailed,
    ] {
        reply.result = Ste100Result::ComponentFailed {
            code,
            detail: "fixture component failure".into(),
        };
        assert_eq!(validate_reply(&reply, &request, &fixture_scheme()), Ok(()));
    }
    reply.result = Ste100Result::ComponentFailed {
        code: Ste100ComponentCode::NoResult,
        detail: String::new(),
    };
    assert!(validate_reply(&reply, &request, &fixture_scheme()).is_err());
    reply.result = Ste100Result::NotSte100 {
        code: Ste100DefectCode::NonSte100,
        detail: "defect".into(),
        spans: vec![span(5, 9)],
    };
    assert!(validate_reply(&reply, &request, &fixture_scheme()).is_err());
}

#[test]
fn template_results_require_canonical_target_and_empty_bindings() {
    let mut request = request(1, 0);
    request.target = Ste100Target::Template {
        template_id: "fixture read".into(),
    };
    request.text = serde_json::to_string(&TemplateSpec {
        template_id: "fixture read".into(),
        kind: "loop.instruction".into(),
        parts: vec![TemplatePart::Literal("Read file.".into())],
    })
    .unwrap();
    let mut reply = accepted(&request);
    bindings(&mut reply).clear();
    assert!(validate_reply(&reply, &request, &fixture_scheme()).is_ok());
    bindings(&mut reply).push(binding());
    assert!(validate_reply(&reply, &request, &fixture_scheme()).is_err());
    bindings(&mut reply).clear();
    request.text.push(' ');
    assert!(validate_reply(&reply, &request, &fixture_scheme()).is_err());
}

fn readable(port: &impl Ste100Port) -> bool {
    use nix::poll::{PollFd, PollFlags, poll};
    let mut fds = [PollFd::new(port.readiness(), PollFlags::POLLIN)];
    poll(&mut fds, 0_u16).unwrap() != 0
}

#[test]
fn scripted_port_returns_busy_ownership_and_one_completion_after_fragments() {
    let request = request(1, 0);
    let reply = accepted(&request);
    let mut bytes = serde_json::to_vec(&reply).unwrap();
    bytes.push(b'\n');
    let split = bytes.len() / 2;
    let (mut port, mut peer) = ScriptedPort::fixture([
        ScriptStep::Hold,
        ScriptStep::Bytes(bytes[..split].to_vec()),
        ScriptStep::Bytes(bytes[split..].to_vec()),
    ])
    .unwrap();
    assert_eq!(port.submit(request.clone()), Ok(Submit::Submitted));
    let unsent = super::tests::request(2, 1);
    assert_eq!(
        port.submit(unsent.clone()),
        Ok(Submit::Busy(unsent.clone()))
    );
    assert_eq!(port.try_result(), Ok(ResultPoll::Pending));
    peer.advance().unwrap();
    assert!(!readable(&port));
    peer.advance().unwrap();
    assert!(readable(&port));
    assert_eq!(port.try_result(), Ok(ResultPoll::Pending));
    peer.advance().unwrap();
    assert!(readable(&port));
    assert_eq!(port.try_result(), Ok(ResultPoll::Completed(reply)));
    assert_eq!(
        port.try_result(),
        Ok(ResultPoll::Pending),
        "completion transfers once"
    );
    // The adapter's request slot is released by Completed itself. The
    // coordinator, tested separately, must guard its held reply until Recorded.
    assert_eq!(port.submit(unsent.clone()), Ok(Submit::Submitted));
    assert_eq!(peer.submitted(), vec![request, unsent]);
}

#[test]
fn scripted_port_distinguishes_peer_loss_from_malformed_reply() {
    for (step, expected) in [
        (ScriptStep::Close, Ste100ComponentCode::Crashed),
        (
            ScriptStep::Bytes(b"{\"score\":0.99}\n".to_vec()),
            Ste100ComponentCode::InvalidResult,
        ),
        (
            ScriptStep::Bytes(b"{\"request_id\":\"1\",\"request_id\":\"1\"}\n".to_vec()),
            Ste100ComponentCode::InvalidResult,
        ),
    ] {
        let (mut port, mut peer) = ScriptedPort::fixture([step]).unwrap();
        port.submit(request(1, 0)).unwrap();
        peer.advance().unwrap();
        let failure = port.try_result().unwrap_err();
        assert_eq!(failure.kind, PortFailureKind::Component(expected));
        assert_eq!(port.try_result(), Err(failure.clone()));
        assert_eq!(port.submit(request(2, 0)), Err(failure));
    }
}

#[test]
fn scripted_port_has_its_own_counter_and_exact_scheme() {
    for mut request in [request(1, 0), request(2, 0)] {
        if request.request_id == DecimalU64(1) {
            request.scheme_id = "another".into();
        }
        let (mut port, _) = ScriptedPort::fixture([]).unwrap();
        assert_eq!(
            port.submit(request).unwrap_err().kind,
            PortFailureKind::ProtocolFailed
        );
    }
    assert!(
        fixture_scheme().validate().is_err(),
        "fixture scheme is never a production declaration"
    );
    assert_eq!(scripted::FIXTURE_ARTIFACT, b"fixture-read/1\nread\n");
    assert_eq!(
        fixture_scheme().artifact_sha256.as_str(),
        scripted::FIXTURE_SHA256
    );
}

fn guard(
    steps: impl IntoIterator<Item = ScriptStep>,
) -> (Classifier<ScriptedPort>, scripted::ScriptedPeer) {
    let (port, peer) = ScriptedPort::fixture(steps).unwrap();
    (Classifier::fixture(port, "source run".into()), peer)
}

fn recorded(reply: &Ste100Reply) -> super::super::RecordedSte100 {
    super::super::RecordedSte100::for_test_in("source run", reply)
}

#[test]
fn coordinator_holds_completion_until_the_exact_result_is_recorded() {
    let first = request(1, 0);
    let reply = accepted(&first);
    let (mut classifier, mut peer) = guard([ScriptStep::Reply(reply.clone())]);
    assert_eq!(classifier.state(), ClassifierState::Idle);
    assert_eq!(classifier.next_request_id(), Ok(DecimalU64(1)));
    assert_eq!(classifier.submit(first.clone()), Ok(Submit::Submitted));
    assert_eq!(classifier.state(), ClassifierState::InFlight);
    assert_eq!(classifier.outstanding(), Some(&first));
    assert_eq!(classifier.next_request_id(), Ok(DecimalU64(2)));
    let next = request(2, 1);
    assert_eq!(
        classifier.submit(next.clone()),
        Ok(Submit::Busy(next.clone()))
    );
    assert_eq!(classifier.try_result(), Ok(ClassifierPoll::Pending));
    peer.advance().unwrap();
    assert_eq!(classifier.try_result(), Ok(ClassifierPoll::ResultHeld));
    assert_eq!(classifier.state(), ClassifierState::ResultHeld);
    assert_eq!(classifier.held(), Some(&reply));
    // Repeated trace capacity waits do not release this reply or issue request
    // 2, even though the adapter has released its own slot on Completed.
    for _ in 0..3 {
        assert_eq!(
            classifier.submit(next.clone()),
            Ok(Submit::Busy(next.clone()))
        );
        assert_eq!(classifier.held(), Some(&reply));
        assert_eq!(peer.submitted(), vec![first.clone()]);
        assert_eq!(classifier.next_request_id(), Ok(DecimalU64(2)));
    }
    assert_eq!(classifier.recorded(recorded(&reply)), Ok(Some(reply)));
    assert_eq!(classifier.state(), ClassifierState::Idle);
    assert!(classifier.held().is_none());
    assert!(classifier.outstanding().is_none());
    assert_eq!(classifier.submit(next.clone()), Ok(Submit::Submitted));
    assert_eq!(peer.submitted(), vec![first, next]);
}

#[test]
fn accepted_input_grant_consumes_the_exact_recorded_result_once() {
    let mut request = request(1, 0);
    request.target = Ste100Target::Input {
        input_id: 4.into(),
        block: 0.into(),
    };
    let reply = accepted(&request);
    let (mut classifier, mut peer) = guard([ScriptStep::Reply(reply.clone())]);
    classifier.submit(request.clone()).unwrap();
    peer.advance().unwrap();
    classifier.try_result().unwrap();
    let grant = classifier
        .recorded_input(recorded(&reply), 2.into())
        .unwrap()
        .unwrap();
    assert_eq!(grant.run(), "source run");
    assert_eq!(grant.offered_sequence(), DecimalU64(2));
    assert_eq!(grant.text(), request.text);
    assert_eq!(grant.scheme_id(), request.scheme_id);
    assert_eq!(grant.bindings(), &[binding()]);
    assert_eq!(grant.request(), &request);
    assert_eq!(grant.reply(), &reply);
    assert_eq!(classifier.state(), ClassifierState::Idle);
    assert!(classifier.outstanding().is_none() && classifier.held().is_none());
    assert!(
        classifier
            .recorded_input(recorded(&reply), 2.into())
            .is_err()
    );
}

#[test]
fn interrupted_release_retains_the_exact_proof_and_input_grant_until_one_later_probe() {
    let mut first = request(1, 0);
    first.target = Ste100Target::Input {
        input_id: 4.into(),
        block: 0.into(),
    };
    let reply = accepted(&first);
    let (mut classifier, mut peer) = guard([ScriptStep::Reply(reply.clone())]);
    classifier.submit(first.clone()).unwrap();
    peer.advance().unwrap();
    classifier.try_result().unwrap();
    classifier.interrupt_terminal_probes_for_test(3);
    assert!(
        classifier
            .recorded_input(recorded(&reply), 2.into())
            .unwrap()
            .is_none()
    );
    assert_eq!(classifier.interrupted_terminal_probes, 2);
    for remaining in [1, 0] {
        assert!(
            classifier
                .resume_recorded_input(2.into())
                .unwrap()
                .is_none()
        );
        assert_eq!(classifier.interrupted_terminal_probes, remaining);
        assert!(classifier.terminal_interrupted);
        let proof = classifier.recorded_proof.as_ref().unwrap();
        assert_eq!(proof.run(), "source run");
        assert!(proof.matches(&reply));
        assert_eq!(classifier.held(), Some(&reply));
        assert_eq!(classifier.outstanding(), Some(&first));
        assert_eq!(classifier.state(), ClassifierState::ResultHeld);
        assert!(classifier.failure().is_none());
        let next = request(2, 1);
        assert_eq!(classifier.submit(next.clone()), Ok(Submit::Busy(next)));
    }
    // An ordinary transport probe may become clear, but cannot consume the
    // retained recording proof or mint the caller's input grant.
    classifier.check_held_terminal().unwrap();
    assert!(!classifier.terminal_interrupted);
    assert!(classifier.recorded_proof.is_some());
    assert_eq!(classifier.held(), Some(&reply));
    let grant = classifier.resume_recorded_input(2.into()).unwrap().unwrap();
    assert_eq!(grant.request(), &first);
    assert_eq!(grant.reply(), &reply);
    assert_eq!(grant.offered_sequence(), 2.into());
    assert_eq!(peer.submitted(), vec![first]);
    assert!(classifier.held().is_none());
    assert!(classifier.outstanding().is_none());
    assert!(classifier.recorded_proof.is_none());
    assert!(classifier.resume_recorded_input(2.into()).is_err());
}

#[test]
fn interrupted_recorded_acceptance_or_rejection_cannot_release_after_peer_loss() {
    for rejected in [false, true] {
        for terminal in [ScriptStep::Close, ScriptStep::ShutdownWrite] {
            let first = request(1, 0);
            let mut reply = accepted(&first);
            if rejected {
                reply.result = Ste100Result::NotSte100 {
                    code: Ste100DefectCode::NonSte100,
                    detail: "correct language rejection".into(),
                    spans: vec![span(5, 10)],
                };
            }
            let (mut classifier, mut peer) = guard([ScriptStep::Reply(reply.clone()), terminal]);
            classifier.submit(first.clone()).unwrap();
            peer.advance().unwrap();
            classifier.try_result().unwrap();
            classifier.interrupt_terminal_probes_for_test(1);
            assert_eq!(classifier.recorded(recorded(&reply)), Ok(None));
            peer.advance().unwrap();
            await_terminal(classifier.readiness(), libc::POLLHUP | libc::POLLRDHUP);
            assert_eq!(
                classifier.resume_recorded().unwrap_err().kind,
                PortFailureKind::Component(Ste100ComponentCode::Crashed)
            );
            assert_eq!(classifier.state(), ClassifierState::Failed);
            assert_eq!(classifier.held(), Some(&reply));
            assert_eq!(classifier.outstanding(), Some(&first));
            assert!(classifier.recorded_proof.as_ref().unwrap().matches(&reply));
            assert_eq!(peer.submitted(), vec![first]);
        }
    }
}

#[test]
fn a_second_recording_proof_cannot_replace_an_interrupted_release() {
    let first = request(1, 0);
    let reply = accepted(&first);
    let (mut classifier, mut peer) = guard([ScriptStep::Reply(reply.clone())]);
    classifier.submit(first.clone()).unwrap();
    peer.advance().unwrap();
    classifier.try_result().unwrap();
    classifier.interrupt_terminal_probes_for_test(1);
    assert_eq!(classifier.recorded(recorded(&reply)), Ok(None));
    // This second proof also matches the exact reply and run. It still cannot
    // replace the already-owned recording boundary or release its completion.
    let original = classifier.recorded_proof.as_ref().unwrap().source().clone();
    assert_eq!(
        classifier.recorded(recorded(&reply)).unwrap_err().kind,
        PortFailureKind::ProtocolFailed
    );
    assert_eq!(classifier.state(), ClassifierState::Failed);
    assert_eq!(
        classifier.recorded_proof.as_ref().unwrap().source(),
        &original
    );
    assert_eq!(classifier.held(), Some(&reply));
    assert_eq!(classifier.outstanding(), Some(&first));
    assert_eq!(peer.submitted(), vec![first]);
}

#[test]
fn component_failure_release_never_waits_for_a_terminal_probe() {
    let first = request(1, 0);
    let mut reply = accepted(&first);
    reply.result = Ste100Result::ComponentFailed {
        code: Ste100ComponentCode::NoResult,
        detail: "original terminal failure".into(),
    };
    let (mut classifier, mut peer) = guard([ScriptStep::Reply(reply.clone()), ScriptStep::Close]);
    classifier.submit(first.clone()).unwrap();
    peer.advance().unwrap();
    classifier.try_result().unwrap();
    peer.advance().unwrap();
    classifier.interrupt_terminal_probes_for_test(3);
    assert_eq!(classifier.recorded(recorded(&reply)), Ok(Some(reply)));
    assert_eq!(classifier.interrupted_terminal_probes, 3);
    assert_eq!(classifier.state(), ClassifierState::Failed);
    assert_eq!(classifier.outstanding(), Some(&first));
    assert_eq!(
        classifier.failure().unwrap().kind,
        PortFailureKind::Component(Ste100ComponentCode::NoResult)
    );
}

#[test]
fn accepted_input_grant_refuses_foreign_or_changed_recording_proofs() {
    for foreign_run in [false, true] {
        let mut request = request(1, 0);
        request.target = Ste100Target::Input {
            input_id: 4.into(),
            block: 0.into(),
        };
        let reply = accepted(&request);
        let (mut classifier, mut peer) = guard([ScriptStep::Reply(reply.clone())]);
        classifier.submit(request).unwrap();
        peer.advance().unwrap();
        classifier.try_result().unwrap();
        let proof = if foreign_run {
            super::super::RecordedSte100::for_test_in("another run", &reply)
        } else {
            let mut changed = reply.clone();
            changed.request_id = 9.into();
            recorded(&changed)
        };
        assert!(classifier.recorded_input(proof, 2.into()).is_err());
        assert_eq!(classifier.state(), ClassifierState::Failed);
        assert_eq!(classifier.held(), Some(&reply));
    }
}

#[test]
fn an_accepted_source_or_nonzero_input_block_cannot_mint_an_input_grant() {
    for source_target in [true, false] {
        let mut request = request(1, 0);
        if !source_target {
            request.target = Ste100Target::Input {
                input_id: 4.into(),
                block: 1.into(),
            };
        }
        let reply = accepted(&request);
        let (mut classifier, mut peer) = guard([ScriptStep::Reply(reply.clone())]);
        classifier.submit(request).unwrap();
        peer.advance().unwrap();
        classifier.try_result().unwrap();
        assert!(
            classifier
                .recorded_input(recorded(&reply), 2.into())
                .is_err()
        );
        assert_eq!(classifier.held(), Some(&reply));
    }
}

#[test]
fn wrong_recording_proofs_cannot_release_or_replace_the_held_result() {
    for cross_run in [false, true] {
        let mut first = request(1, 0);
        // Input targets carry no run in their own wire shape. The guard's run
        // identity must still reject an otherwise identical cross-run receipt.
        first.target = Ste100Target::Input {
            input_id: 1.into(),
            block: 0.into(),
        };
        let reply = accepted(&first);
        let (mut classifier, mut peer) = guard([ScriptStep::Reply(reply.clone())]);
        classifier.submit(first.clone()).unwrap();
        peer.advance().unwrap();
        classifier.try_result().unwrap();
        let proof = if cross_run {
            super::super::RecordedSte100::for_test_in("other run", &reply)
        } else {
            let mut changed = reply.clone();
            bindings(&mut changed)[0].nouns[0].value = "changed".into();
            recorded(&changed)
        };
        assert_eq!(
            classifier.recorded(proof).unwrap_err().kind,
            PortFailureKind::ProtocolFailed
        );
        assert_eq!(classifier.state(), ClassifierState::Failed);
        assert_eq!(classifier.held(), Some(&reply));
        assert_eq!(classifier.outstanding(), Some(&first));
        assert!(classifier.submit(request(2, 1)).is_err());
        assert_eq!(peer.submitted(), vec![first]);
    }
}

#[test]
fn no_result_is_recorded_before_terminal_failure_and_rejection_is_not_failure() {
    for code in [
        None,
        Some(Ste100ComponentCode::NoResult),
        Some(Ste100ComponentCode::Crashed),
        Some(Ste100ComponentCode::InvalidResult),
        Some(Ste100ComponentCode::TransportFailed),
    ] {
        let first = request(1, 0);
        let mut reply = accepted(&first);
        reply.result = match code {
            Some(code) => Ste100Result::ComponentFailed {
                code,
                detail: "terminal fixture failure".into(),
            },
            None => Ste100Result::NotSte100 {
                code: Ste100DefectCode::NonSte100,
                detail: "correct language rejection".into(),
                spans: vec![span(5, 10)],
            },
        };
        let (mut classifier, mut peer) = guard([ScriptStep::Reply(reply.clone())]);
        classifier.submit(first).unwrap();
        peer.advance().unwrap();
        assert_eq!(classifier.try_result(), Ok(ClassifierPoll::ResultHeld));
        assert_eq!(classifier.state(), ClassifierState::ResultHeld);
        assert!(classifier.failure().is_none());
        assert_eq!(classifier.recorded(recorded(&reply)), Ok(Some(reply)));
        match code {
            Some(code) => {
                assert_eq!(classifier.state(), ClassifierState::Failed);
                assert_eq!(
                    classifier.failure().unwrap().kind,
                    PortFailureKind::Component(code)
                );
                assert!(classifier.submit(request(2, 1)).is_err());
            }
            None => {
                assert_eq!(classifier.state(), ClassifierState::Idle);
                assert!(classifier.failure().is_none());
                assert_eq!(classifier.submit(request(2, 1)), Ok(Submit::Submitted));
            }
        }
    }
}

#[test]
fn a_complete_component_failure_survives_the_required_peer_close_until_recorded() {
    for code in [
        Ste100ComponentCode::NoResult,
        Ste100ComponentCode::Crashed,
        Ste100ComponentCode::InvalidResult,
        Ste100ComponentCode::TransportFailed,
    ] {
        for half_close in [false, true] {
            let first = request(1, 0);
            let mut reply = accepted(&first);
            let detail = format!("original terminal cause: {code:?}");
            reply.result = Ste100Result::ComponentFailed {
                code,
                detail: detail.clone(),
            };
            let close = if half_close {
                ScriptStep::ShutdownWrite
            } else {
                ScriptStep::Close
            };
            let (mut classifier, mut peer) = guard([ScriptStep::Reply(reply.clone()), close]);
            classifier.submit(first.clone()).unwrap();
            peer.advance().unwrap();
            assert_eq!(classifier.try_result(), Ok(ClassifierPoll::ResultHeld));
            peer.advance().unwrap();

            // The complete terminal reply is the cause. PORT's required close
            // cannot replace it with a new crash or bypass its trace record.
            assert_eq!(classifier.check_held_terminal(), Ok(()));
            assert_eq!(classifier.state(), ClassifierState::ResultHeld);
            assert_eq!(classifier.held(), Some(&reply));
            assert_eq!(classifier.outstanding(), Some(&first));
            assert!(classifier.failure().is_none());
            let next = request(2, 1);
            assert_eq!(classifier.submit(next.clone()), Ok(Submit::Busy(next)));
            assert_eq!(peer.submitted(), vec![first.clone()]);

            assert_eq!(classifier.recorded(recorded(&reply)), Ok(Some(reply)));
            assert_eq!(classifier.state(), ClassifierState::Failed);
            let failure = classifier.failure().unwrap();
            assert_eq!(failure.kind, PortFailureKind::Component(code));
            assert_eq!(failure.detail, detail);
            assert_eq!(classifier.outstanding(), Some(&first));
            assert!(classifier.held().is_none());
            assert!(classifier.submit(request(2, 1)).is_err());
            assert_eq!(peer.submitted(), vec![first]);
        }
    }
}

#[test]
fn unsolicited_results_fail_idle_and_held_without_erasing_the_account() {
    let first = request(1, 0);
    let reply = accepted(&first);
    let extra = accepted(&request(2, 1));
    let (mut classifier, mut peer) = guard([ScriptStep::Reply(reply.clone())]);
    peer.advance().unwrap();
    let failure = classifier.try_result().unwrap_err();
    assert_eq!(failure.kind, PortFailureKind::ProtocolFailed);
    assert_eq!(failure.received, Some(Box::new(reply.clone())));
    assert_eq!(classifier.state(), ClassifierState::Failed);
    assert!(peer.submitted().is_empty());

    let (mut classifier, mut peer) = guard([
        ScriptStep::Reply(reply.clone()),
        ScriptStep::Reply(extra.clone()),
    ]);
    classifier.submit(first.clone()).unwrap();
    peer.advance().unwrap();
    classifier.try_result().unwrap();
    peer.advance().unwrap();
    let failure = classifier.try_result().unwrap_err();
    assert_eq!(failure.kind, PortFailureKind::ProtocolFailed);
    assert_eq!(failure.received, Some(Box::new(extra)));
    assert_eq!(classifier.state(), ClassifierState::Failed);
    assert_eq!(classifier.held(), Some(&reply));
    assert_eq!(classifier.outstanding(), Some(&first));
    assert!(classifier.recorded(recorded(&reply)).is_err());
    assert_eq!(classifier.state(), ClassifierState::Failed);
    assert_eq!(classifier.outstanding(), Some(&first));
    assert_eq!(classifier.held(), Some(&reply));
}

#[test]
fn a_coalesced_extra_reply_remains_readable_while_the_first_is_held() {
    let first = request(1, 0);
    let reply = accepted(&first);
    let extra = accepted(&request(2, 1));
    let mut bytes = serde_json::to_vec(&reply).unwrap();
    bytes.push(b'\n');
    bytes.extend(serde_json::to_vec(&extra).unwrap());
    bytes.push(b'\n');
    let (mut classifier, mut peer) = guard([ScriptStep::Bytes(bytes)]);
    classifier.submit(first).unwrap();
    peer.advance().unwrap();
    assert_eq!(classifier.try_result(), Ok(ClassifierPoll::ResultHeld));
    assert_eq!(classifier.check_held_terminal(), Ok(()));
    assert_eq!(classifier.state(), ClassifierState::ResultHeld);
    assert_eq!(classifier.held(), Some(&reply));
    use nix::poll::{PollFd, PollFlags, poll};
    let mut fds = [PollFd::new(classifier.readiness(), PollFlags::POLLIN)];
    assert_eq!(poll(&mut fds, 0_u16).unwrap(), 1);
    assert_eq!(
        classifier.try_result().unwrap_err().received,
        Some(Box::new(extra))
    );
    assert_eq!(classifier.held(), Some(&reply));
}

// Parallel process fixtures can inherit the peer between fork and exec. Closing
// this thread's peer handle then need not make HUP visible in the same instant.
// Wait only for the fixture's kernel precondition, without calling the classifier
// or consuming bytes. Its production probe still gets exactly one call below.
fn await_terminal(fd: BorrowedFd<'_>, expected: i16) {
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(3);
    loop {
        let remaining = deadline
            .checked_duration_since(std::time::Instant::now())
            .expect("fixture peer did not become terminal within its watchdog");
        let timeout = i32::try_from(remaining.as_millis().saturating_add(1).min(100))
            .expect("bounded fixture poll timeout");
        let mut status = libc::pollfd {
            fd: fd.as_raw_fd(),
            events: libc::POLLRDHUP,
            revents: 0,
        };
        // SAFETY: status is initialized writable storage and fd stays borrowed.
        // Each wait is capped and the original test watchdog never restarts.
        let result = unsafe { libc::poll(&mut status, 1, timeout) };
        if result < 0 {
            assert_eq!(
                std::io::Error::last_os_error().kind(),
                std::io::ErrorKind::Interrupted,
            );
            continue;
        }
        if status.revents & expected != 0 {
            return;
        }
        assert_eq!(status.revents, 0, "unexpected fixture descriptor status");
    }
}

#[test]
fn peer_death_after_a_held_acceptance_cannot_release_success() {
    for terminal in [ScriptStep::Close, ScriptStep::ShutdownWrite] {
        let first = request(1, 0);
        let reply = accepted(&first);
        let (mut classifier, mut peer) = guard([ScriptStep::Reply(reply.clone()), terminal]);
        classifier.submit(first.clone()).unwrap();
        peer.advance().unwrap();
        assert_eq!(classifier.try_result(), Ok(ClassifierPoll::ResultHeld));
        peer.advance().unwrap();
        await_terminal(classifier.readiness(), libc::POLLHUP | libc::POLLRDHUP);
        assert_eq!(
            classifier.check_held_terminal().unwrap_err().kind,
            PortFailureKind::Component(Ste100ComponentCode::Crashed)
        );
        assert!(classifier.recorded(recorded(&reply)).is_err());
        assert_eq!(classifier.state(), ClassifierState::Failed);
        assert_eq!(classifier.held(), Some(&reply));
        assert_eq!(classifier.outstanding(), Some(&first));
        assert!(classifier.submit(request(2, 1)).is_err());
        assert_eq!(peer.submitted(), vec![first]);
    }
}

#[test]
fn recording_itself_observes_peer_loss_before_releasing_the_held_result() {
    for terminal in [ScriptStep::Close, ScriptStep::ShutdownWrite] {
        let first = request(1, 0);
        let reply = accepted(&first);
        let (mut classifier, mut peer) = guard([ScriptStep::Reply(reply.clone()), terminal]);
        classifier.submit(first.clone()).unwrap();
        peer.advance().unwrap();
        assert_eq!(classifier.try_result(), Ok(ClassifierPoll::ResultHeld));
        peer.advance().unwrap();
        await_terminal(classifier.readiness(), libc::POLLHUP | libc::POLLRDHUP);
        // No transport call or try_result occurs between loss and release.
        assert_eq!(
            classifier.recorded(recorded(&reply)).unwrap_err().kind,
            PortFailureKind::Component(Ste100ComponentCode::Crashed)
        );
        assert_eq!(classifier.state(), ClassifierState::Failed);
        assert_eq!(classifier.held(), Some(&reply));
        assert_eq!(classifier.outstanding(), Some(&first));
    }
}

#[test]
fn a_language_rejection_cannot_release_after_peer_loss() {
    for terminal in [ScriptStep::Close, ScriptStep::ShutdownWrite] {
        let first = request(1, 0);
        let mut reply = accepted(&first);
        reply.result = Ste100Result::NotSte100 {
            code: Ste100DefectCode::NonSte100,
            detail: "correct language rejection".into(),
            spans: vec![span(5, 10)],
        };
        let (mut classifier, mut peer) = guard([ScriptStep::Reply(reply.clone()), terminal]);
        classifier.submit(first.clone()).unwrap();
        peer.advance().unwrap();
        assert_eq!(classifier.try_result(), Ok(ClassifierPoll::ResultHeld));
        peer.advance().unwrap();
        await_terminal(classifier.readiness(), libc::POLLHUP | libc::POLLRDHUP);
        assert_eq!(
            classifier.recorded(recorded(&reply)).unwrap_err().kind,
            PortFailureKind::Component(Ste100ComponentCode::Crashed)
        );
        assert_eq!(classifier.state(), ClassifierState::Failed);
        assert_eq!(classifier.held(), Some(&reply));
        assert_eq!(classifier.outstanding(), Some(&first));
    }
}

#[test]
fn descriptor_error_freezes_a_held_result_without_polling_the_adapter() {
    use std::os::fd::{AsFd, OwnedFd};

    struct ErrorPort {
        readiness: OwnedFd,
        reply: Option<Ste100Reply>,
    }
    impl Ste100Port for ErrorPort {
        fn readiness(&self) -> BorrowedFd<'_> {
            self.readiness.as_fd()
        }
        fn submit(&mut self, _: Ste100Request) -> Result<Submit, PortFailure> {
            Ok(Submit::Submitted)
        }
        fn try_result(&mut self) -> Result<ResultPoll, PortFailure> {
            Ok(ResultPoll::Completed(
                self.reply
                    .take()
                    .expect("the held slot prevents another read"),
            ))
        }
    }

    // The write end of a pipe reports POLLERR when its read end closes. It is
    // owned throughout the probe, so this exercises a real descriptor error
    // without fabricating an invalid BorrowedFd.
    let (reader, writer) = nix::unistd::pipe().unwrap();
    let first = request(1, 0);
    let reply = accepted(&first);
    let port = ErrorPort {
        readiness: writer,
        reply: Some(reply.clone()),
    };
    let mut classifier = Classifier::fixture(port, "source run".into());
    classifier.submit(first.clone()).unwrap();
    classifier.try_result().unwrap();
    drop(reader);
    await_terminal(classifier.readiness(), libc::POLLERR);
    assert_eq!(
        classifier.check_held_terminal().unwrap_err().kind,
        PortFailureKind::Component(Ste100ComponentCode::TransportFailed)
    );
    assert_eq!(classifier.state(), ClassifierState::Failed);
    assert_eq!(classifier.held(), Some(&reply));
    assert_eq!(classifier.outstanding(), Some(&first));
    assert!(classifier.recorded(recorded(&reply)).is_err());
}

#[test]
fn malformed_or_mismatched_results_and_eof_preserve_the_outstanding_request() {
    let first = request(1, 0);
    let mut wrong = accepted(&first);
    wrong.target = Ste100Target::Input {
        input_id: 4.into(),
        block: 0.into(),
    };
    for step in [
        ScriptStep::Reply(wrong),
        ScriptStep::Bytes(b"{broken}\n".to_vec()),
        ScriptStep::Close,
    ] {
        let (mut classifier, mut peer) = guard([step]);
        classifier.submit(first.clone()).unwrap();
        peer.advance().unwrap();
        assert!(classifier.try_result().is_err());
        assert_eq!(classifier.state(), ClassifierState::Failed);
        assert_eq!(classifier.outstanding(), Some(&first));
        assert!(classifier.held().is_none());
        assert!(classifier.submit(request(2, 0)).is_err());
        assert_eq!(peer.submitted(), vec![first.clone()]);
    }
}

#[test]
fn external_failure_and_close_freeze_every_active_state() {
    for initial in [
        ClassifierState::Idle,
        ClassifierState::InFlight,
        ClassifierState::ResultHeld,
    ] {
        for close in [false, true] {
            let first = request(1, 0);
            let reply = accepted(&first);
            let (mut classifier, mut peer) = guard([ScriptStep::Reply(reply.clone())]);
            if initial != ClassifierState::Idle {
                classifier.submit(first.clone()).unwrap();
            }
            if initial == ClassifierState::ResultHeld {
                peer.advance().unwrap();
                classifier.try_result().unwrap();
            }
            assert_eq!(classifier.state(), initial);
            if close {
                classifier.close();
            } else {
                classifier.fail();
            }
            assert_eq!(
                classifier.state(),
                if close {
                    ClassifierState::Closed
                } else {
                    ClassifierState::Failed
                }
            );
            assert!(classifier.submit(request(2, 1)).is_err());
            assert!(classifier.try_result().is_err());
            assert_eq!(
                peer.submitted().len(),
                usize::from(initial != ClassifierState::Idle)
            );
            assert_eq!(
                classifier.held(),
                (initial == ClassifierState::ResultHeld).then_some(&reply)
            );
            if initial == ClassifierState::ResultHeld {
                assert!(classifier.recorded(recorded(&reply)).is_err());
                assert_eq!(classifier.held(), Some(&reply));
                assert_eq!(classifier.outstanding(), Some(&first));
            }
            if close {
                classifier.fail();
                assert_eq!(classifier.state(), ClassifierState::Closed);
            }
        }
    }
}

#[test]
fn counter_exhaustion_foreign_source_and_fixture_scheme_fail_locally() {
    let (mut classifier, peer) = guard([]);
    classifier.next_id = None;
    assert_eq!(
        classifier.next_request_id().unwrap_err().kind,
        PortFailureKind::ProtocolFailed
    );
    assert_eq!(classifier.state(), ClassifierState::Failed);
    assert!(peer.submitted().is_empty());
    let (mut classifier, peer) = guard([]);
    let mut foreign = request(1, 0);
    let Ste100Target::Source(source) = &mut foreign.target else {
        unreachable!()
    };
    source.run = "foreign run".into();
    assert_eq!(
        classifier.submit(foreign).unwrap_err().kind,
        PortFailureKind::ProtocolFailed
    );
    assert!(peer.submitted().is_empty());
    let (port, _) = ScriptedPort::fixture([]).unwrap();
    assert!(Classifier::new(port, fixture_scheme(), "source run".into()).is_err());
}

#[test]
fn submitted_source_targets_keep_the_callers_order_without_a_scheduler() {
    let (mut classifier, mut peer) = guard([]);
    let mut submitted = Vec::new();
    for (id, block) in [(1, 0), (2, 1), (3, 2)] {
        let request = request(id, block);
        let reply = accepted(&request);
        peer.push(ScriptStep::Reply(reply.clone()));
        classifier.submit(request.clone()).unwrap();
        peer.advance().unwrap();
        classifier.try_result().unwrap();
        classifier.recorded(recorded(&reply)).unwrap().unwrap();
        submitted.push(request);
    }
    assert_eq!(peer.submitted(), submitted);
}

#[test]
fn checked_constructor_accepts_only_the_held_production_scheme_identity() {
    let mut scheme = fixture_scheme();
    scheme.scheme_id = format!("ste100:{}", scheme.artifact_sha256.as_str());
    let (port, _) = ScriptedPort::checked(&scheme, []).unwrap();
    let mut classifier = Classifier::new(port, scheme.clone(), "source run".into()).unwrap();
    let mut request = request(1, 0);
    request.scheme_id = scheme.scheme_id;
    assert_eq!(classifier.submit(request), Ok(Submit::Submitted));
    // The readiness descriptor is borrowed from the actual adapter.
    use std::os::fd::AsRawFd;
    assert!(classifier.readiness().as_raw_fd() >= 0);
}
