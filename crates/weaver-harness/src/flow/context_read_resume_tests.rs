//! Resuming a consumed read completion alongside real selected-source work.

use super::super::super::coordinator::TransportSource;
use super::super::super::ste100_channel::{Channel, Receive, Tag};
use super::super::input_tests::InputFixture;
use super::*;

fn source_in_flight() -> (Member, Channel, s::Ste100Request) {
    let mut input = InputFixture::new();
    input
        .run
        .author(AuthoredEvent {
            kind: Kind::MessageAssistant,
            subsystem: Subsystem::Harness,
            turn: Some(TurnRef("earlier-turn".into())),
            source: super::super::super::author::AuthoredSource::Model,
            causal_parent: Some(Sequence(0)),
            payload: Some(Payload::Message(
                serde_json::value::to_raw_value(&serde_json::json!({
                    "role": "assistant",
                    "content": [{"type": "text", "text": "Read earlier."}]
                }))
                .unwrap(),
            )),
        })
        .unwrap();
    let request = (0..128)
        .find_map(|_| {
            input.run.context.reduce_flow(&mut input.run.clock).unwrap();
            input.classifier_transport();
            match input.peer.try_receive().unwrap() {
                Receive::Complete(message) => {
                    assert_eq!(message.tag, Tag::Request);
                    Some(
                        s::decode_strict::<s::Ste100Request>(
                            std::str::from_utf8(&message.payload).unwrap(),
                        )
                        .unwrap(),
                    )
                }
                Receive::Pending | Receive::Fragment => None,
            }
        })
        .expect("the selected source crosses the actual classifier socket");
    assert!(matches!(request.target, s::Ste100Target::Source(_)));
    assert!(input.run.context.flow.pending_check().is_none());
    // Settle the real ClassificationStarted observation, keeping the classifier
    // exchange and selected source pending throughout the subsequent read.
    for _ in 0..16 {
        input.run.context.reduce_flow(&mut input.run.clock).unwrap();
        if !input.run.context.flow.has_pending_observations() {
            break;
        }
    }
    assert!(!input.run.context.flow.has_pending_observations());
    let InputFixture { run, peer } = input;
    (
        Member {
            run,
            next_operation: 2,
            requests: Vec::new(),
        },
        peer,
        request,
    )
}

fn released_answer_yields(member: &mut Member) -> AbsoluteBound {
    let initial = member.begin();
    let bound = member.bound();
    member.respond(&initial, page(3, 0, EMPTY.len(), EMPTY));
    let release = member.next_request();
    member.respond(
        &release,
        s::StateControlAnswer::Released {
            snapshot_id: 3.into(),
        },
    );
    member.record_completion_only();
    let now = member.now();
    let leave = AbsoluteBound::after(now, 10).unwrap();
    member
        .run
        .context
        .arm(PumpDeadline::Leave, leave, &mut member.run.clock)
        .unwrap();
    let mut clock = ValidationClock {
        before: now,
        deadline: leave.deadline_ns(),
        reads: 0,
    };
    assert_eq!(
        member
            .run
            .context
            .settle_recorded_read(&mut clock)
            .unwrap_err(),
        RunFailure::Wait {
            key: PumpDeadline::Leave,
            bound: leave
        }
    );
    assert!(member.run.context.flow.operation_completion().is_none());
    assert!(member.run.context.flow.state().outstanding().is_empty());
    assert!(!member.run.context.read_blocks_reduction());
    assert_eq!(member.bound(), bound);
    assert_eq!(
        member.run.context.complete_wait(PumpDeadline::Leave),
        Some(leave)
    );
    member.run.clock.advance_to(leave.deadline_ns()).unwrap();
    bound
}

fn hold_classifier_reply(
    member: &mut Member,
    peer: &mut Channel,
    request: &s::Ste100Request,
) -> s::Ste100Reply {
    let reply = s::Ste100Reply {
        request_id: request.request_id,
        target: request.target.clone(),
        scheme_id: request.scheme_id.clone(),
        result: s::Ste100Result::Accepted {
            bindings: vec![s::Binding {
                statement_span: s::Span {
                    start: 0.into(),
                    end: 13.into(),
                },
                verb: "read".into(),
                verb_span: s::Span {
                    start: 0.into(),
                    end: 4.into(),
                },
                condition: None,
                condition_span: None,
                nouns: vec![s::NounBinding {
                    value: "earlier".into(),
                    span: s::Span {
                        start: 5.into(),
                        end: 12.into(),
                    },
                }],
            }],
        },
    };
    peer.begin_send(
        Tag::Reply,
        reply.request_id.0,
        serde_json::to_vec(&reply).unwrap(),
    )
    .unwrap();
    for _ in 0..64 {
        peer.try_send().unwrap();
        if !peer.has_pending_send() {
            break;
        }
    }
    assert!(!peer.has_pending_send());
    member
        .run
        .context
        .flow
        .transport_one(
            &mut FlowRecording {
                barrier: &mut member.run.context.barrier,
                inbox: &mut member.run.context.inbox,
                budget: &mut member.run.context.budget,
                author: &member.run.context.author,
            },
            &mut member.run.clock,
            TransportSource::Classifier,
        )
        .unwrap();
    assert!(member.records(Kind::Ste100Result).is_empty());
    reply
}

fn classifier_after_release(cancel: bool) {
    let (mut member, mut peer, request) = source_in_flight();
    let bound = released_answer_yields(&mut member);
    let reply = hold_classifier_reply(&mut member, &mut peer, &request);
    if cancel {
        member
            .run
            .context
            .cancel_read(&mut member.run.clock)
            .unwrap();
    }
    member.fill_trace();
    assert_eq!(
        member
            .advance()
            .expect("resumed release must retain recorder ownership"),
        ReadProgress::Pending
    );
    assert_eq!(member.bound(), bound);
    assert_eq!(
        member.run.context.pending_author().unwrap().event.kind,
        Kind::Recall
    );
    assert!(member.records(Kind::Ste100Result).is_empty());
    member.no_result();
    recall_tests::drain_admitted(&mut member);
    assert_eq!(member.advance().unwrap(), ReadProgress::Complete);
    let completion = member
        .run
        .context
        .take_read(&mut member.run.clock)
        .unwrap()
        .unwrap();
    let sequence = match completion {
        ReadCompletion::Recorded(read) if !cancel => {
            assert_eq!(read.raw_bytes(), EMPTY);
            assert_eq!(read.snapshot_id(), 3.into());
            read.recall_sequence()
        }
        ReadCompletion::Cancelled(read) if cancel => {
            assert!(read.refusal().is_none());
            read.recall_sequence()
                .expect("cancelled success still owes recall")
        }
        other => panic!("read lost its requested disposition: {other:?}"),
    };
    assert_eq!(member.records(Kind::Recall).len(), 1);
    assert_eq!(
        member.records(Kind::Recall)[0]["sequence"],
        sequence.0.to_string()
    );
    assert!(
        member
            .run
            .context
            .deadlines
            .get(PumpDeadline::StateControl)
            .is_none()
    );
    member.no_result();
    member.no_request();
    // The deferred completion must still belong to its exact source/request.
    for _ in 0..16 {
        member
            .run
            .context
            .reduce_flow(&mut member.run.clock)
            .unwrap();
        if !member.records(Kind::Ste100Result).is_empty() {
            break;
        }
    }
    let results = member.records(Kind::Ste100Result);
    assert_eq!(results.len(), 1);
    assert_eq!(results[0]["payload"], serde_json::to_value(&reply).unwrap());
    assert!(
        results[0]["sequence"]
            .as_str()
            .unwrap()
            .parse::<u64>()
            .unwrap()
            > sequence.0
    );
    assert_eq!(member.records(Kind::Recall).len(), 1);
    assert_eq!(member.requests.len(), 2);
    assert!(member.run.context.failure().is_none());
}

#[test]
fn accepted_release_resumes_before_a_held_classifier_record() {
    classifier_after_release(false);
}

#[test]
fn cancelled_accepted_release_records_before_a_held_classifier_result() {
    classifier_after_release(true);
}

struct YieldAtBoundary {
    before: u64,
    deadline: u64,
    boundary: usize,
    reads: usize,
}

impl PumpIo for YieldAtBoundary {
    fn now(&mut self) -> std::io::Result<u64> {
        self.reads += 1;
        Ok(if self.reads < self.boundary {
            self.before
        } else {
            self.deadline
        })
    }

    fn wait(&mut self, _: &[WaitInterest<'_>], _: Option<u64>) -> std::io::Result<WaitOutcome> {
        panic!("read reduction must not block");
    }
}

#[test]
fn accepted_release_keeps_recall_custody_across_observation_tick_yields() {
    // Sweep actual clock checkpoints rather than pinning a private call count.
    // Before the fix, a second yield can leave a real Sampled notice queued
    // alongside the consumed Released continuation; its next author steals
    // recorder ownership when pressure is applied below.
    for boundary in 1..32 {
        let (mut member, _silent_peer, _) = source_in_flight();
        let bound = released_answer_yields(&mut member);
        let context = &mut member.run.context;
        let tick = context
            .flow
            .observation_wake(&mut FlowRecording {
                barrier: &mut context.barrier,
                inbox: &mut context.inbox,
                budget: &mut context.budget,
                author: &context.author,
            })
            .unwrap()
            .expect("the selected source still awaits its classifier");
        member.run.clock.advance_to(tick).unwrap();
        let label = AbsoluteBound::after(tick, 10).unwrap();
        member
            .run
            .context
            .arm(PumpDeadline::Label, label, &mut member.run.clock)
            .unwrap();
        let mut clock = YieldAtBoundary {
            before: tick,
            deadline: label.deadline_ns(),
            boundary,
            reads: 0,
        };
        let progress = member.run.context.advance_read(&mut ports(&mut clock));
        let reached = clock.reads >= boundary;
        if reached {
            assert_eq!(
                progress.unwrap_err(),
                RunFailure::Wait {
                    key: PumpDeadline::Label,
                    bound: label
                }
            );
        } else {
            assert!(progress.is_ok(), "{progress:?}");
        }
        if member.run.context.flow.has_pending_observations() {
            eprintln!("real sampled observation retained at clock boundary {boundary}");
        }
        assert_eq!(
            member.run.context.complete_wait(PumpDeadline::Label),
            Some(label)
        );
        member.run.clock.advance_to(label.deadline_ns()).unwrap();
        member.fill_trace();
        let progress = member
            .advance()
            .expect("a yielded read must retain recall ownership ahead of an observation");
        if progress != ReadProgress::Complete {
            assert_eq!(member.bound(), bound);
            member.no_result();
        }
        recall_tests::drain_admitted(&mut member);
        let read = member.finish().unwrap();
        assert_eq!(read.raw_bytes(), EMPTY);
        assert_eq!(read.snapshot_id(), 3.into());
        assert_eq!(member.records(Kind::Recall).len(), 1);
        assert_eq!(
            member.records(Kind::Recall)[0]["sequence"],
            read.recall_sequence().0.to_string()
        );
        assert_eq!(member.requests.len(), 2);
        assert!(member.records(Kind::Ste100Result).is_empty());
        assert!(member.run.context.failure().is_none());
        member.no_result();
        member.no_request();
        // The due observation remains possible once recall has released custody.
        for _ in 0..16 {
            member
                .run
                .context
                .reduce_flow(&mut member.run.clock)
                .unwrap();
            if member
                .records(Kind::StateFlow)
                .iter()
                .any(|record| record["payload"]["phase"] == "sampled")
            {
                break;
            }
        }
        let samples: Vec<_> = member
            .records(Kind::StateFlow)
            .into_iter()
            .filter(|record| record["payload"]["phase"] == "sampled")
            .collect();
        assert_eq!(samples.len(), 1);
        assert!(
            samples[0]["sequence"]
                .as_str()
                .unwrap()
                .parse::<u64>()
                .unwrap()
                > read.recall_sequence().0
        );
        if !reached {
            return;
        }
    }
    panic!("clock boundary sweep did not reach an uninterrupted read advance");
}
