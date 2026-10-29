use super::*;
use std::io::Read;
use std::os::unix::net::UnixStream;
use std::time::Duration;

use weaver_trace::serving::{
    Channel, Envelope, Event, Origin, PreflightAttempt, PreflightRecorder, validate_partition,
};
use weaver_trace::{MonotonicNs, RunRef, SessionRef};
use weaver_types::state_flow::{
    Binding, FlowBacklog, NounBinding, Span, Ste100DefectCode, Ste100Request,
};

use crate::flow::classifier::scripted::{ScriptStep, ScriptedPort};
use crate::flow::classifier::{Classifier, ClassifierPoll, Submit};
use crate::flow::intake::{FlowInbox, RetentionBudget};
use crate::flow::recording::{RecordBarrier, RecordProgress, RecordProof, RecordPurpose};
use crate::flow::{mirror, recording_fixture};

// A second applicable trait makes this expression ambiguous if Clone appears.
const _: fn() = || {
    trait AcceptedInputMustNotClone<Marker> {
        fn check() {}
    }
    impl<T: ?Sized> AcceptedInputMustNotClone<()> for T {}
    impl<T: Clone> AcceptedInputMustNotClone<u8> for T {}
    let _ = <AcceptedInput as AcceptedInputMustNotClone<_>>::check;
};

const TEXT: &str = "Read caf\u{e9}.";
const SCHEME: &str = "fixture-read/1";

fn input_refusal(slot: &InputSlot) -> InputRefusal {
    let Some(Refusal::Input(refusal)) = slot.refusal() else {
        panic!("expected a shape or language refusal");
    };
    refusal
}

#[test]
fn no_residency_precedes_parsing_and_waits_for_its_record_before_each_close() {
    let mut slot = input_slot();
    for (index, octets) in [b"\xffnot JSON".as_slice(), br#"{"text":"Read file."}"#]
        .into_iter()
        .enumerate()
    {
        let frame = TurnFrame::carry_from(octets, 42);
        let offered = Sequence(10 + index as u64 * 2);
        slot.begin(frame.clone(), bound()).unwrap();
        assert!(slot.refusal().is_none());
        slot.offered_recorded(offered, true).unwrap();
        assert_eq!(slot.frame(), Some(&frame));
        assert!(slot.current.as_ref().unwrap().text.is_none());
        assert_eq!(slot.refusal(), Some(Refusal::NoResidency));
        assert_eq!(
            slot.refusal().unwrap().record(),
            RefusalRecord::Lifecycle {
                asked: LifecycleAsk::WorldRequest,
                refusal: LifecycleRefusal::NoResidency
            }
        );
        assert!(slot.check_operation().is_none());
        assert!(slot.take_accepted().is_none());
        assert!(slot.close_frame().is_none());
        assert!(!slot.gate_may_disarm());
        slot.refusal_recorded(Sequence(offered.0 + 1)).unwrap();
        assert!(slot.gate_may_disarm());
        assert_eq!(
            slot.close_frame().unwrap(),
            TurnFrame::carry(br#"{"kind":"refused","reason":"no model is resident"}"#)
        );
        assert!(slot.finish().is_err());
        slot.response_sent().unwrap();
        slot.finish().unwrap();
    }
}

#[test]
fn no_residency_can_replace_only_an_unsubmitted_uncancelled_input() {
    let mut slot = input_slot();
    offered(&mut slot);
    slot.refuse_no_residency();
    assert_eq!(slot.refusal(), Some(Refusal::NoResidency));
    assert!(slot.check_operation().is_none());
    slot.cancel().unwrap();
    slot.refuse_no_residency();
    assert_eq!(slot.snapshot().unwrap().state, InputState::Cancelled);
    slot.refusal_recorded(Sequence(2)).unwrap();
    assert_eq!(
        slot.close_frame().unwrap(),
        TurnFrame::carry(br#"{"kind":"stopped","reason":"The agent is leaving."}"#)
    );

    let mut slot = input_slot();
    let _operation = checking(&mut slot);
    let before = slot.snapshot();
    slot.refuse_no_residency();
    assert_eq!(slot.snapshot(), before);
    assert!(slot.refusal().is_none());
    assert!(slot.close_frame().is_none());
    assert!(!slot.gate_may_disarm());

    let mut slot = input_slot();
    slot.begin(frame(), bound()).unwrap();
    slot.cancel().unwrap();
    slot.offered_recorded(Sequence(1), true).unwrap();
    assert_eq!(slot.snapshot().unwrap().state, InputState::Cancelled);
    assert!(slot.refusal().is_none());
}

fn input_slot() -> InputSlot {
    InputSlot::new("run".into(), SCHEME.into()).unwrap()
}

fn bound() -> AbsoluteBound {
    AbsoluteBound::after(71, 1000).unwrap()
}

fn frame() -> TurnFrame {
    TurnFrame::carry_from(
        &serde_json::to_vec(&serde_json::json!({"text": TEXT})).unwrap(),
        1001,
    )
}

fn offered(slot: &mut InputSlot) {
    slot.begin(frame(), bound()).unwrap();
    slot.offered_recorded(Sequence(1), false).unwrap();
}

fn checking(slot: &mut InputSlot) -> CheckOperation {
    offered(slot);
    let operation = slot.check_operation().unwrap();
    slot.submitted(1.into()).unwrap();
    operation
}

fn request(operation: &CheckOperation) -> Ste100Request {
    let CheckOperation::Input { input_id, text, .. } = operation else {
        panic!("input operation")
    };
    Ste100Request {
        request_id: 1.into(),
        target: Ste100Target::Input {
            input_id: *input_id,
            block: 0.into(),
        },
        text: text.clone(),
        scheme_id: SCHEME.into(),
    }
}

fn accepted(request: &Ste100Request) -> Ste100Reply {
    let span = |start: u64, end: u64| Span {
        start: start.into(),
        end: end.into(),
    };
    Ste100Reply {
        request_id: request.request_id,
        target: request.target.clone(),
        scheme_id: request.scheme_id.clone(),
        result: Ste100Result::Accepted {
            bindings: vec![Binding {
                statement_span: span(0, 11),
                verb: "read".into(),
                verb_span: span(0, 4),
                condition: None,
                condition_span: None,
                nouns: vec![NounBinding {
                    value: "caf\u{e9}".into(),
                    span: span(5, 10),
                }],
            }],
        },
    }
}

fn rejected(operation: &CheckOperation) -> Ste100Reply {
    let request = request(operation);
    Ste100Reply {
        request_id: request.request_id,
        target: request.target,
        scheme_id: request.scheme_id,
        result: Ste100Result::NotSte100 {
            code: Ste100DefectCode::NonSte100,
            detail: "the exact classified defect".into(),
            spans: vec![],
        },
    }
}

fn event(kind: Kind, payload: Payload) -> Event {
    Event {
        envelope: Envelope {
            session: SessionRef("session".into()),
            run: RunRef("run".into()),
            turn: None,
            sequence: Sequence(0),
            kind,
            channel: if kind == Kind::InputOffered {
                Channel::Input
            } else {
                Channel::Internal
            },
            origin: if kind == Kind::InputOffered {
                Origin::External
            } else {
                Origin::Runtime
            },
            subsystem: Subsystem::Harness,
            causal_parent: None,
            wall_ms: 1,
            monotonic_ns: MonotonicNs(7),
        },
        payload: Some(payload),
    }
}

/// The fixture supplies language bindings, but the grant is minted by the
/// actual classifier only after the actual recorder returns its typed proof.
fn recorded_grant(operation: &CheckOperation) -> (Ste100Reply, AcceptedReuse) {
    let request = request(operation);
    let reply = accepted(&request);
    let (port, mut peer) = ScriptedPort::fixture([ScriptStep::Reply(reply.clone())]).unwrap();
    let mut classifier = Classifier::fixture(port, "run".into());
    assert_eq!(classifier.submit(request).unwrap(), Submit::Submitted);
    assert!(peer.advance().unwrap());
    assert_eq!(classifier.try_result().unwrap(), ClassifierPoll::ResultHeld);

    let (sink, mut reader) = UnixStream::pair().unwrap();
    reader
        .set_read_timeout(Some(Duration::from_secs(5)))
        .unwrap();
    let drain = std::thread::spawn(move || {
        let mut bytes = Vec::new();
        reader.read_to_end(&mut bytes).unwrap();
        bytes
    });
    let preflight = PreflightRecorder::receive(
        sink.into(),
        RunRef("run".into()),
        SessionRef("session".into()),
    )
    .unwrap();
    let PreflightAttempt::Recorded { recorder, .. } = preflight
        .record_load(event(
            Kind::Load,
            Payload::Elections(Box::new(recording_fixture::load_record())),
        ))
        .unwrap_or_else(|error| panic!("load: {:?}", error.failure))
    else {
        panic!("load capacity")
    };
    let recorder = recorder.activate(validate_partition(&recording_fixture::partition()).unwrap());
    let mut barrier = RecordBarrier::new(recorder, "run".into());
    let mut inbox = FlowInbox::new();
    let mut budget = RetentionBudget::new(FlowBacklog {
        max_pending_blocks: 2.into(),
        max_pending_bytes: 100_000.into(),
        lag_alarm_turns: 2.into(),
        measurement_ref: "input transaction fixture".into(),
    })
    .unwrap();
    let CheckOperation::Input {
        input_id,
        offered_sequence,
        ..
    } = operation
    else {
        unreachable!()
    };
    assert!(matches!(
        barrier.submit(
            event(
                Kind::InputOffered,
                Payload::InputOffered(Box::new(InputOfferedRecord {
                    input_id: input_id.0.into(),
                    octets: frame().octets,
                }))
            ),
            RecordPurpose::Observation,
            &mut inbox,
            &mut budget
        ),
        RecordProgress::Recorded {
            sequence: Sequence(1),
            ..
        }
    ));
    let progress = barrier.submit(
        event(
            Kind::Ste100Result,
            Payload::Ste100Result(Box::new(mirror::ste100(&reply))),
        ),
        RecordPurpose::Classifier(reply.clone()),
        &mut inbox,
        &mut budget,
    );
    let RecordProgress::Recorded {
        proof: Some(RecordProof::Classifier(proof)),
        ..
    } = progress
    else {
        panic!("classifier result must record: {progress:?}")
    };
    let grant = classifier
        .recorded_input(proof, *offered_sequence)
        .unwrap()
        .unwrap();
    drop(barrier);
    let bytes = drain.join().unwrap();
    let lines: Vec<serde_json::Value> = std::str::from_utf8(&bytes)
        .unwrap()
        .lines()
        .map(|line| serde_json::from_str(line).unwrap())
        .collect();
    assert_eq!(
        lines
            .iter()
            .filter(|line| line["kind"] == "ste100.result")
            .count(),
        1
    );
    (reply, grant)
}

fn accepted_slot() -> InputSlot {
    let mut slot = input_slot();
    let operation = checking(&mut slot);
    let (reply, grant) = recorded_grant(&operation);
    slot.accept_completion(operation, reply, Some(grant))
        .unwrap();
    slot
}

#[test]
fn original_frame_is_offered_before_any_decode_or_shape_refusal() {
    let mut slot = input_slot();
    let frame = TurnFrame {
        octets: "not base64 at all".into(),
        dialer: Some(4002),
    };
    slot.begin(frame.clone(), bound()).unwrap();
    assert_eq!(slot.frame(), Some(&frame));
    assert_eq!(slot.snapshot().unwrap().state, InputState::Offered);
    assert!(slot.refusal().is_none());
    assert!(slot.check_operation().is_none());
    let event = slot.offered_event().unwrap();
    assert_eq!(event.kind, Kind::InputOffered);
    assert!(matches!(event.source, AuthoredSource::BoundaryInput(4002)));
    assert!(event.turn.is_none());
    let Some(Payload::InputOffered(payload)) = event.payload else {
        panic!("offered payload")
    };
    assert_eq!(payload.input_id.0, 1);
    assert_eq!(payload.octets, frame.octets);
    slot.offered_recorded(Sequence(8), false).unwrap();
    assert!(slot.offered_event().is_none());
    assert_eq!(
        input_refusal(&slot).case,
        InputRefusalCase::InvalidInputShape
    );
    assert!(!slot.gate_may_disarm());
    assert!(slot.close_frame().is_none());
    assert!(slot.finish().is_err());
    slot.refusal_recorded(Sequence(9)).unwrap();
    assert!(slot.refusal().is_none());
    assert!(slot.gate_may_disarm());
    let close = slot.close_frame().unwrap();
    assert_eq!(close.dialer, None);
    assert_eq!(
        close.octets().unwrap(),
        br#"{"kind":"refused","reason":"Restate the input in STE100."}"#
    );
    assert!(slot.finish().is_err());
    slot.response_sent().unwrap();
    assert!(slot.close_frame().is_none());
    slot.finish().unwrap();
    assert!(slot.snapshot().is_none());
}

#[test]
fn every_role_and_malformed_shape_is_refused_without_a_check() {
    let cases: &[&[u8]] = &[
        br#"{"text":"Read file.","role":"user"}"#,
        br#"{"text":"Read file.","role":"system"}"#,
        br#"{"text":"Read file.","role":null}"#,
        br#"{"text":"Read file.","role":17}"#,
        br#"{"text":"Read file.","text":"Read other."}"#,
        br#"{"text":"Read file.","role":null,"role":null}"#,
        br#"{"text":"Read file.","extra":true}"#,
        br#"{"text":17}"#,
        br#"{}"#,
        b"\xff",
        b"[1]",
        b"null",
    ];
    for bytes in cases {
        let mut slot = input_slot();
        slot.begin(TurnFrame::carry_from(bytes, 1001), bound())
            .unwrap();
        slot.offered_recorded(Sequence(1), false).unwrap();
        assert_eq!(
            input_refusal(&slot).case,
            InputRefusalCase::InvalidInputShape,
            "{bytes:?}"
        );
        assert!(slot.check_operation().is_none(), "{bytes:?}");
        assert!(slot.take_accepted().is_none(), "{bytes:?}");
    }
}

#[test]
fn refused_intake_preserves_the_caller_and_does_not_spend_an_id() {
    let mut slot = input_slot();
    let untrusted = TurnFrame::carry(b"{}");
    let error = slot.begin(untrusted.clone(), bound()).unwrap_err();
    assert_eq!(error.error, InputError::MissingDialer);
    assert_eq!(error.frame, untrusted);
    assert_eq!(error.bound, bound());
    assert!(slot.snapshot().is_none());
    slot.begin(frame(), bound()).unwrap();
    assert_eq!(slot.snapshot().unwrap().input_id.0, 1);
    let second = TurnFrame::carry_from(b"second", 3);
    let error = slot.begin(second.clone(), bound()).unwrap_err();
    assert_eq!(error.error, InputError::Busy);
    assert_eq!(error.frame, second);
    assert_eq!(slot.frame(), Some(&frame()));
}

#[test]
fn accepted_recorded_input_is_consumed_once_with_exact_bytes_and_identity() {
    let mut slot = accepted_slot();
    assert!(slot.gate_may_disarm());
    assert!(slot.finish().is_err());
    let token = slot.take_accepted().unwrap();
    assert!(slot.take_accepted().is_none());
    let failure = slot
        .consume_input(token, "another-run", SCHEME)
        .unwrap_err();
    assert_eq!(failure.error, InputError::ForeignIdentity);
    let failure = slot
        .consume_input(failure.token, "run", "another-scheme")
        .unwrap_err();
    assert_eq!(failure.error, InputError::ForeignIdentity);
    let mut input = slot.consume_input(failure.token, "run", SCHEME).unwrap();
    assert_eq!(
        input.message,
        Message {
            role: Role::User,
            content: vec![ContentBlock::Text { text: TEXT.into() }]
        }
    );
    assert_eq!(input.input_id.0, 1);
    assert_eq!(input.offered_sequence.0, 1);
    assert_eq!(input.dialer, 1001);
    assert_eq!(input.reuse.take().unwrap().text(), TEXT);
    assert_eq!(slot.cancel(), Err(InputError::InvalidTransition));
    assert!(slot.close_frame().is_none());
    slot.finish().unwrap();
    slot.begin(frame(), bound()).unwrap();
    assert_eq!(slot.snapshot().unwrap().input_id.0, 2);
}

#[test]
fn acceptance_requires_the_actual_recorded_grant() {
    let mut slot = input_slot();
    let operation = checking(&mut slot);
    let reply = accepted(&request(&operation));
    let before = slot.snapshot();
    let error = slot
        .accept_completion(operation.clone(), reply.clone(), None)
        .unwrap_err();
    assert_eq!(error.error, InputError::MissingAcceptedProof);
    assert_eq!(error.operation, operation);
    assert_eq!(error.reply, reply);
    assert_eq!(slot.snapshot(), before);
    assert!(slot.take_accepted().is_none());
    assert!(!slot.gate_may_disarm());
}

#[test]
fn every_completion_identity_is_checked_before_consuming_evidence() {
    type Change = fn(&mut CheckOperation, &mut Ste100Reply);
    let changes: &[(&str, Change)] = &[
        ("input", |op, _| {
            if let CheckOperation::Input { input_id, .. } = op {
                *input_id = 2.into();
            }
        }),
        ("offered", |op, _| {
            if let CheckOperation::Input {
                offered_sequence, ..
            } = op
            {
                *offered_sequence = 2.into();
            }
        }),
        ("text", |op, _| {
            if let CheckOperation::Input { text, .. } = op {
                text.push(' ');
            }
        }),
        ("request", |_, reply| reply.request_id = 2.into()),
        ("target", |_, reply| {
            reply.target = Ste100Target::Input {
                input_id: 2.into(),
                block: 0.into(),
            }
        }),
        ("block", |_, reply| {
            reply.target = Ste100Target::Input {
                input_id: 1.into(),
                block: 1.into(),
            }
        }),
        ("scheme", |_, reply| reply.scheme_id.push('x')),
    ];
    for (name, change) in changes {
        let mut slot = input_slot();
        let mut operation = checking(&mut slot);
        let (mut reply, grant) = recorded_grant(&operation);
        change(&mut operation, &mut reply);
        let before = slot.snapshot();
        let error = slot
            .accept_completion(operation.clone(), reply.clone(), Some(grant))
            .unwrap_err();
        assert_eq!(error.error, InputError::Correlation, "{name}");
        assert_eq!(error.operation, operation, "{name}");
        assert_eq!(error.reply, reply, "{name}");
        assert!(error.reuse.is_some(), "{name}");
        assert_eq!(slot.snapshot(), before, "{name}");
        assert!(slot.take_accepted().is_none(), "{name}");
    }
}

#[test]
fn correct_rejection_waits_for_its_refusal_record_and_never_mints_a_token() {
    let mut slot = input_slot();
    let operation = checking(&mut slot);
    let reply = rejected(&operation);
    slot.accept_completion(operation.clone(), reply.clone(), None)
        .unwrap();
    assert_eq!(slot.current.as_ref().unwrap().reply.as_ref(), Some(&reply));
    let refusal = input_refusal(&slot);
    assert_eq!(refusal.detail, "the exact classified defect");
    assert_eq!(refusal.case, InputRefusalCase::NonSte100);
    assert_eq!(
        slot.refusal_recorded(Sequence(1)),
        Err(InputError::InvalidTransition)
    );
    assert!(!slot.gate_may_disarm());
    assert!(slot.take_accepted().is_none());
    assert!(slot.close_frame().is_none());
    slot.refusal_recorded(Sequence(4)).unwrap();
    assert!(slot.gate_may_disarm());
    assert!(slot.accept_completion(operation, reply, None).is_err());
    assert!(slot.refusal_recorded(Sequence(5)).is_err());
    assert!(slot.take_accepted().is_none());
}

#[test]
fn cancellation_before_submit_keeps_the_original_offer_and_deadline() {
    for before_offer_record in [true, false] {
        let mut slot = input_slot();
        slot.begin(frame(), bound()).unwrap();
        if !before_offer_record {
            slot.offered_recorded(Sequence(1), false).unwrap();
        }
        slot.cancel().unwrap();
        assert!(slot.check_operation().is_none());
        assert!(slot.submitted(1.into()).is_err());
        assert!(!slot.gate_may_disarm());
        if before_offer_record {
            assert!(slot.offered_event().is_some());
            assert!(slot.close_frame().is_none());
            slot.offered_recorded(Sequence(1), false).unwrap();
        }
        assert_eq!(
            slot.close_frame().unwrap().octets().unwrap(),
            br#"{"kind":"stopped","reason":"The agent is leaving."}"#
        );
        assert!(slot.finish().is_err());
        slot.response_sent().unwrap();
        assert!(slot.gate_may_disarm());
        assert!(slot.close_frame().is_none());
        assert_eq!(slot.snapshot().unwrap().bound, bound());
        slot.finish().unwrap();
    }
}

#[test]
fn cancelled_submitted_check_keeps_its_exchange_until_recorded_late_result() {
    for accepts in [true, false] {
        let mut slot = input_slot();
        let operation = checking(&mut slot);
        slot.cancel().unwrap();
        slot.response_sent().unwrap();
        assert!(!slot.gate_may_disarm());
        assert!(slot.finish().is_err());
        assert_eq!(slot.snapshot().unwrap().request_id, Some(1.into()));
        assert_eq!(slot.snapshot().unwrap().bound, bound());
        let (reply, grant) = if accepts {
            let (reply, grant) = recorded_grant(&operation);
            (reply, Some(grant))
        } else {
            (rejected(&operation), None)
        };
        slot.accept_completion(operation, reply.clone(), grant)
            .unwrap();
        assert_eq!(slot.current.as_ref().unwrap().reply.as_ref(), Some(&reply));
        assert!(slot.take_accepted().is_none());
        assert!(slot.refusal().is_none());
        assert!(slot.gate_may_disarm());
        assert_eq!(slot.snapshot().unwrap().state, InputState::Cancelled);
        slot.finish().unwrap();
    }
}

#[test]
fn cancellation_and_halt_revoke_an_already_transferred_token() {
    for halt in [false, true] {
        let mut slot = accepted_slot();
        let token = slot.take_accepted().unwrap();
        if halt {
            slot.halt();
            slot.halt();
        } else {
            slot.cancel().unwrap();
        }
        let error = slot.consume_input(token, "run", SCHEME).unwrap_err();
        assert_eq!(error.error, InputError::Revoked);
        assert!(slot.take_accepted().is_none());
        assert_eq!(slot.frame(), Some(&frame()));
        assert_eq!(slot.snapshot().unwrap().request_id, Some(1.into()));
        assert_eq!(slot.snapshot().unwrap().bound, bound());
        assert!(slot.finish().is_err());
        if halt {
            assert!(slot.snapshot().unwrap().halted);
            assert_eq!(slot.snapshot().unwrap().state, InputState::Accepted);
            assert!(!slot.gate_may_disarm());
            assert_eq!(
                slot.close_frame().unwrap().octets().unwrap(),
                br#"{"kind":"stopped","reason":"The agent stopped because state flow failed."}"#
            );
            assert_eq!(
                slot.begin(frame(), bound()).unwrap_err().error,
                InputError::Halted
            );
        }
    }
}

#[test]
fn halt_retains_pending_evidence_and_never_reopens_or_reanswers() {
    let mut slot = input_slot();
    let operation = checking(&mut slot);
    let snapshot = slot.snapshot().unwrap();
    slot.halt();
    assert_eq!(
        slot.snapshot().unwrap(),
        InputSnapshot {
            halted: true,
            ..snapshot
        }
    );
    let error = slot
        .accept_completion(operation.clone(), rejected(&operation), None)
        .unwrap_err();
    assert_eq!(error.error, InputError::Halted);
    assert_eq!(error.operation, operation);
    assert!(slot.close_frame().is_some());
    slot.response_sent().unwrap();
    assert!(slot.close_frame().is_none());
    assert!(!slot.gate_may_disarm());
    assert!(slot.finish().is_err());

    let mut slot = input_slot();
    let operation = checking(&mut slot);
    slot.accept_completion(operation.clone(), rejected(&operation), None)
        .unwrap();
    slot.refusal_recorded(Sequence(3)).unwrap();
    slot.response_sent().unwrap();
    slot.halt();
    assert!(
        slot.close_frame().is_none(),
        "an answered exchange cannot send a second close"
    );
}

#[test]
fn input_counter_stops_at_its_checked_limit_without_wrapping() {
    let mut slot = input_slot();
    slot.next_id = Some(u64::MAX);
    slot.begin(frame(), bound()).unwrap();
    assert_eq!(slot.snapshot().unwrap().input_id.0, u64::MAX);
    slot.offered_recorded(Sequence(1), false).unwrap();
    slot.cancel().unwrap();
    slot.response_sent().unwrap();
    slot.finish().unwrap();
    let error = slot.begin(frame(), bound()).unwrap_err();
    assert_eq!(error.error, InputError::CounterExhausted);
    assert_eq!(error.frame, frame());
    assert!(slot.snapshot().is_none());
}

#[test]
fn consumed_input_never_emits_an_unnamed_close_after_halt() {
    let mut slot = accepted_slot();
    let token = slot.take_accepted().unwrap();
    let input = slot.consume_input(token, "run", SCHEME).unwrap();
    slot.halt();
    assert!(slot.close_frame().is_none());
    assert_eq!(slot.response_sent(), Err(InputError::InvalidTransition));
    assert_eq!(slot.finish(), Err(InputError::Halted));
    assert_eq!(input.message.role, Role::User);
}

#[test]
fn recorded_cancelled_result_disarms_before_a_backpressured_close() {
    let mut slot = input_slot();
    let operation = checking(&mut slot);
    slot.cancel().unwrap();
    let (reply, grant) = recorded_grant(&operation);
    slot.accept_completion(operation, reply, Some(grant))
        .unwrap();
    assert!(slot.gate_may_disarm());
    assert!(slot.close_frame().is_some());
    assert!(slot.take_accepted().is_none());
    assert!(slot.finish().is_err());
    assert_eq!(slot.snapshot().unwrap().bound, bound());
    slot.response_sent().unwrap();
    slot.finish().unwrap();
}

#[test]
fn lost_response_keeps_checks_owed_and_never_claims_a_send() {
    let mut slot = input_slot();
    let operation = checking(&mut slot);
    assert_eq!(slot.response_lost(), Err(InputError::InvalidTransition));
    slot.cancel().unwrap();
    slot.response_lost().unwrap();
    assert!(!slot.snapshot().unwrap().response_sent);
    assert_eq!(slot.snapshot().unwrap().bound, bound());
    assert!(
        slot.finish().is_err(),
        "lost connection does not settle an owed check"
    );
    let (reply, grant) = recorded_grant(&operation);
    slot.accept_completion(operation, reply, Some(grant))
        .unwrap();
    assert!(!slot.snapshot().unwrap().response_sent);
    assert!(slot.take_accepted().is_none());
    slot.finish().unwrap();

    let mut slot = input_slot();
    let operation = checking(&mut slot);
    slot.accept_completion(operation.clone(), rejected(&operation), None)
        .unwrap();
    assert!(
        slot.response_lost().is_err(),
        "refusal still owes its record"
    );
    slot.refusal_recorded(Sequence(3)).unwrap();
    slot.response_lost().unwrap();
    assert!(!slot.snapshot().unwrap().response_sent);
    slot.finish().unwrap();
}

#[test]
fn a_recorded_grant_for_another_offer_cannot_authorize_this_input() {
    let mut slot = input_slot();
    slot.begin(frame(), bound()).unwrap();
    slot.offered_recorded(Sequence(2), false).unwrap();
    let operation = slot.check_operation().unwrap();
    slot.submitted(1.into()).unwrap();
    let mut other_offer = operation.clone();
    let CheckOperation::Input {
        offered_sequence, ..
    } = &mut other_offer
    else {
        unreachable!()
    };
    *offered_sequence = 1.into();
    let (reply, grant) = recorded_grant(&other_offer);
    let before = slot.snapshot();
    let error = slot
        .accept_completion(operation.clone(), reply.clone(), Some(grant))
        .unwrap_err();
    assert_eq!(error.error, InputError::MissingAcceptedProof);
    assert_eq!(error.operation, operation);
    assert_eq!(error.reply, reply);
    assert_eq!(error.reuse.unwrap().offered_sequence().0, 1);
    assert_eq!(slot.snapshot(), before);
    assert!(slot.take_accepted().is_none());
}

#[test]
fn cancellation_keeps_a_pending_refusal_author_and_its_cancelled_disposition() {
    for checked in [false, true] {
        let mut slot = input_slot();
        if checked {
            let operation = checking(&mut slot);
            slot.accept_completion(operation.clone(), rejected(&operation), None)
                .unwrap();
        } else {
            slot.begin(TurnFrame::carry_from(b"{}", 1001), bound())
                .unwrap();
            slot.offered_recorded(Sequence(1), false).unwrap();
        }
        let retained_refusal = slot.refusal().unwrap();
        slot.cancel().unwrap();
        assert!(slot.refusal().is_none());
        assert_eq!(slot.gate_may_disarm(), checked);
        slot.refusal_recorded(Sequence(3)).unwrap();
        assert_eq!(slot.snapshot().unwrap().state, InputState::Cancelled);
        assert_eq!(
            slot.current.as_ref().unwrap().refusal.as_ref(),
            Some(&retained_refusal)
        );
        assert_eq!(
            slot.current.as_ref().unwrap().refusal_sequence,
            Some(Sequence(3))
        );
        assert_eq!(
            slot.refusal_recorded(Sequence(4)),
            Err(InputError::InvalidTransition)
        );
        assert_eq!(
            slot.close_frame().unwrap().octets().unwrap(),
            br#"{"kind":"stopped","reason":"The agent is leaving."}"#
        );
        slot.response_sent().unwrap();
        slot.finish().unwrap();
    }
}
