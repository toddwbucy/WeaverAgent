//! Load-template certification through real packet and recorder owners.
//! The compiled fixture inventory and scripted checker are not language qualification.

#[path = "context_template_return_review_tests.rs"]
mod return_review_tests;

use super::super::coordinator::TransportSource;
use super::super::ste100_channel::{Fault, Receive, Tag};
use super::super::templates::{CompiledInventory, TemplateCertificate};
use super::input_tests::InputFixture;
use super::*;
use std::os::fd::AsRawFd;
use std::sync::OnceLock;
use weaver_types::TurnFrame;
use weaver_types::state_flow as s;

fn templates() -> &'static [s::TemplateSpec] {
    static TEMPLATES: OnceLock<Vec<s::TemplateSpec>> = OnceLock::new();
    TEMPLATES.get_or_init(|| {
        vec![
            s::TemplateSpec {
                template_id: "fixture.first".into(),
                kind: "loop.instruction".into(),
                parts: vec![s::TemplatePart::Literal("Read file.".into())],
            },
            s::TemplateSpec {
                template_id: "fixture.technical".into(),
                kind: "loop.instruction".into(),
                parts: vec![
                    s::TemplatePart::Literal("Read ".into()),
                    s::TemplatePart::Slot {
                        name: "item".into(),
                        class: s::SlotClass::TechnicalName,
                    },
                    s::TemplatePart::Literal(".".into()),
                ],
            },
            s::TemplateSpec {
                template_id: "fixture.step".into(),
                kind: "loop.instruction".into(),
                parts: vec![
                    s::TemplatePart::Literal("Read file after ".into()),
                    s::TemplatePart::Slot {
                        name: "step".into(),
                        class: s::SlotClass::StepId,
                    },
                    s::TemplatePart::Literal(".".into()),
                ],
            },
        ]
    })
}

fn inventory() -> CompiledInventory {
    CompiledInventory::new(templates()).unwrap()
}

pub(super) struct TemplateFixture {
    pub(super) input: InputFixture,
}

impl TemplateFixture {
    pub(super) fn new() -> Self {
        Self {
            input: InputFixture::with_template_inventory(&inventory()),
        }
    }

    pub(super) fn with_enter(remaining_enter_ns: u64) -> Self {
        Self {
            input: InputFixture::with_template_inventory_and_enter(
                &inventory(),
                remaining_enter_ns,
            ),
        }
    }

    pub(super) fn begin(&mut self) {
        self.input
            .run
            .context
            .begin_template_checks(inventory(), &mut self.input.run.clock)
            .unwrap();
    }

    pub(super) fn advance(&mut self) -> Result<TemplateProgress, RunFailure> {
        self.input
            .run
            .context
            .advance_template_checks(&mut self.input.run.clock)
    }

    fn request(&mut self, index: usize) -> s::Ste100Request {
        for _ in 0..128 {
            self.advance().unwrap();
            self.input.classifier_transport();
            match self.input.peer.try_receive().unwrap() {
                Receive::Complete(message) => {
                    assert_eq!(message.tag, Tag::Request);
                    let request: s::Ste100Request =
                        s::decode_strict(std::str::from_utf8(&message.payload).unwrap()).unwrap();
                    assert_eq!(message.id, request.request_id.0);
                    assert_eq!(
                        request.target,
                        s::Ste100Target::Template {
                            template_id: templates()[index].template_id.clone(),
                        }
                    );
                    assert_eq!(
                        request.text,
                        serde_json::to_string(&templates()[index]).unwrap()
                    );
                    assert_eq!(
                        request.scheme_id,
                        self.input.run.context.declaration.scheme.scheme_id
                    );
                    return request;
                }
                Receive::Pending | Receive::Fragment => {}
            }
        }
        panic!("template request {index} never crossed the actual socket");
    }

    fn until_accepted(&mut self, count: usize) {
        for _ in 0..128 {
            self.advance().unwrap();
            if self
                .input
                .run
                .context
                .template_transcript()
                .unwrap()
                .accepted_count()
                == count
            {
                return;
            }
        }
        panic!("template transcript did not record {count} acceptances");
    }

    pub(super) fn accept(&mut self, index: usize) -> s::Ste100Reply {
        let request = self.request(index);
        let reply = accepted(&request);
        self.input.reply(&reply);
        self.until_accepted(index + 1);
        reply
    }

    fn certificate(&mut self) -> Result<Option<TemplateCertificate>, RunFailure> {
        self.input
            .run
            .context
            .take_template_certificate(&mut self.input.run.clock)
    }

    fn send_raw(&mut self, id: u64, payload: Vec<u8>) {
        self.input.peer.begin_send(Tag::Reply, id, payload).unwrap();
        for _ in 0..64 {
            self.input.peer.try_send().unwrap();
            if !self.input.peer.has_pending_send() {
                return;
            }
        }
        panic!("fixture reply did not finish");
    }

    fn transport_result(&mut self) {
        let context = &mut self.input.run.context;
        let _ = context.flow.transport_one(
            &mut FlowRecording {
                barrier: &mut context.barrier,
                inbox: &mut context.inbox,
                budget: &mut context.budget,
                author: &context.author,
            },
            &mut self.input.run.clock,
            TransportSource::Classifier,
        );
    }
}

fn accepted(request: &s::Ste100Request) -> s::Ste100Reply {
    s::Ste100Reply {
        request_id: request.request_id,
        target: request.target.clone(),
        scheme_id: request.scheme_id.clone(),
        result: s::Ste100Result::Accepted { bindings: vec![] },
    }
}

fn sequence(record: &serde_json::Value) -> u64 {
    record["sequence"].as_str().unwrap().parse().unwrap()
}

#[test]
fn certification_uses_the_recorded_load_and_ordered_actual_result_sources_once() {
    let mut fx = TemplateFixture::new();
    let enter = fx
        .input
        .run
        .context
        .deadlines
        .get(PumpDeadline::Enter)
        .unwrap();
    let load = fx.input.records(Kind::Load).pop().unwrap();
    fx.begin();
    assert!(fx.certificate().unwrap().is_none());
    let replies: Vec<_> = (0..templates().len())
        .map(|index| fx.accept(index))
        .collect();
    assert_eq!(fx.advance().unwrap(), TemplateProgress::Complete);
    let certificate = fx.certificate().unwrap().unwrap();
    assert_eq!(certificate.run_id(), "run");
    assert_eq!(
        certificate.scheme_id(),
        fx.input.run.context.declaration.scheme.scheme_id
    );
    assert_eq!(certificate.inventory_sha256(), inventory().digest());
    assert_eq!(
        serde_json::to_value(certificate.inventory().record(
            &fx.input.run.context.declaration.scheme,
            certificate.checker_artifact_sha256(),
        ))
        .unwrap(),
        load["payload"]["template_inventory"]
    );
    let records = fx.input.records(Kind::Ste100Result);
    assert_eq!(records.len(), templates().len());
    assert_eq!(certificate.accepted().len(), templates().len());
    for (index, ((grant, reply), record)) in certificate
        .accepted()
        .iter()
        .zip(&replies)
        .zip(&records)
        .enumerate()
    {
        assert_eq!(grant.template_id(), templates()[index].template_id);
        assert_eq!(grant.request_id(), reply.request_id);
        assert_eq!(record["payload"], serde_json::to_value(reply).unwrap());
        assert!(record["turn"].is_null());
        assert_eq!(grant.result_source().block_id.run, "run");
        assert_eq!(grant.result_source().block_id.sequence.0, sequence(record));
        assert_eq!(grant.result_source().block_id.block.0, 0);
        assert!(grant.result_source().turn.is_none());
        assert!(sequence(record) > sequence(&load));
        if index > 0 {
            assert!(sequence(record) > sequence(&records[index - 1]));
            assert!(reply.request_id > replies[index - 1].request_id);
        }
    }
    assert!(fx.certificate().unwrap().is_none());
    assert_eq!(
        fx.input.run.context.deadlines.get(PumpDeadline::Enter),
        Some(enter)
    );
    assert_eq!(fx.input.run.context.budget.gauges(), (0, 0));
    assert!(fx.input.run.context.flow.ledger().facts().next().is_none());
    fx.input.no_request();
}

#[test]
fn a_correct_template_rejection_retains_the_partial_transcript_without_halting_or_next_request() {
    let mut fx = TemplateFixture::new();
    fx.begin();
    let first = fx.accept(0);
    let request = fx.request(1);
    let mut reply = accepted(&request);
    reply.result = s::Ste100Result::NotSte100 {
        code: s::Ste100DefectCode::NonSte100,
        detail: "fixture: symbolic inclusion has a counterexample".into(),
        spans: vec![],
    };
    fx.input.reply(&reply);
    for _ in 0..128 {
        if fx.advance().unwrap() == TemplateProgress::Rejected {
            break;
        }
    }
    assert_eq!(fx.advance().unwrap(), TemplateProgress::Rejected);
    let transcript = fx.input.run.context.template_transcript().unwrap();
    assert_eq!(transcript.accepted_count(), 1);
    assert_eq!(transcript.records().len(), 2);
    assert_eq!(transcript.records()[0].reply(), &first);
    assert_eq!(transcript.rejection(), Some(&reply));
    assert!(!transcript.is_complete());
    assert!(fx.certificate().unwrap().is_none());
    assert!(fx.input.run.context.failure().is_none());
    assert!(fx.input.run.context.halt().is_none());
    assert_eq!(fx.input.run.context.phase(), RecordingPhase::Entered);
    assert_eq!(fx.input.records(Kind::Ste100Result).len(), 2);
    fx.input.no_request();
}

#[test]
fn recorder_pressure_holds_the_exact_reply_and_cannot_certify_or_start_the_next_template() {
    let mut fx = TemplateFixture::new();
    fx.begin();
    let request = fx.request(0);
    fx.input.fill_trace();
    let reply = accepted(&request);
    fx.input.reply(&reply);
    for _ in 0..128 {
        fx.advance().unwrap();
        if fx.input.run.context.pending_author().is_some() {
            break;
        }
    }
    let pending = fx.input.run.context.pending_author().unwrap();
    assert_eq!(pending.event.kind, Kind::Ste100Result);
    let ticket = pending.ticket;
    assert_eq!(
        fx.input
            .run
            .context
            .flow
            .standing_classifier()
            .unwrap()
            .held(),
        Some(&reply)
    );
    assert_eq!(
        fx.input
            .run
            .context
            .template_transcript()
            .unwrap()
            .accepted_count(),
        0
    );
    assert!(fx.input.records(Kind::Ste100Result).is_empty());
    assert!(fx.certificate().unwrap().is_none());
    for _ in 0..4 {
        assert_eq!(fx.advance().unwrap(), TemplateProgress::Pending);
        assert_eq!(
            fx.input.run.context.pending_author().unwrap().ticket,
            ticket
        );
        fx.input.no_request();
    }
    fx.input.drain_trace();
    fx.until_accepted(1);
    assert_eq!(fx.input.records(Kind::Ste100Result).len(), 1);
    assert_eq!(
        fx.input
            .run
            .context
            .template_transcript()
            .unwrap()
            .records()[0]
            .reply(),
        &reply
    );
    fx.accept(1);
    fx.accept(2);
    assert!(fx.certificate().unwrap().is_some());
}

#[test]
fn the_original_enter_deadline_covers_later_template_requests_without_reset() {
    let mut fx = TemplateFixture::new();
    let enter = fx
        .input
        .run
        .context
        .deadlines
        .get(PumpDeadline::Enter)
        .unwrap();
    fx.begin();
    fx.accept(0);
    fx.input
        .run
        .clock
        .advance_to(enter.deadline_ns() - 1)
        .unwrap();
    let second = fx.request(1);
    assert_eq!(
        fx.input.run.context.deadlines.get(PumpDeadline::Enter),
        Some(enter)
    );
    fx.input.run.clock.advance_to(enter.deadline_ns()).unwrap();
    assert_eq!(
        fx.advance().unwrap_err(),
        RunFailure::Wait {
            key: PumpDeadline::Enter,
            bound: enter
        }
    );
    let transcript = fx.input.run.context.template_transcript().unwrap();
    assert_eq!(transcript.accepted_count(), 1);
    assert_eq!(transcript.records().len(), 1);
    assert_eq!(
        fx.input
            .run
            .context
            .flow
            .standing_classifier()
            .unwrap()
            .outstanding()
            .unwrap()
            .request_id,
        second.request_id
    );
    assert!(fx.certificate().is_err());
    assert!(fx.input.run.context.halt().is_none());
    fx.input.no_request();
}

#[test]
fn mismatched_inventory_and_preexisting_input_refuse_without_losing_owned_inputs() {
    let mut fx = TemplateFixture::new();
    let shortened = CompiledInventory::new(&templates()[..1]).unwrap();
    let digest = shortened.digest().clone();
    let error = fx
        .input
        .run
        .context
        .begin_template_checks(shortened, &mut fx.input.run.clock)
        .unwrap_err();
    assert_eq!(error.inventory.digest(), &digest);
    assert!(error.failure.is_none());
    assert!(fx.input.run.context.template_transcript().is_none());
    assert!(fx.input.run.context.failure().is_none());
    fx.input.no_request();

    let mut fx = TemplateFixture::new();
    let frame = TurnFrame::carry_from(br#"{"text":"Read file."}"#, 42);
    fx.input.begin(frame.clone());
    let error = fx
        .input
        .run
        .context
        .begin_template_checks(inventory(), &mut fx.input.run.clock)
        .unwrap_err();
    assert_eq!(error.inventory.digest(), inventory().digest());
    assert!(error.failure.is_none());
    assert!(fx.input.run.context.input.snapshot().is_some());
    assert!(fx.input.run.context.template_transcript().is_none());
    assert!(fx.input.run.context.failure().is_none());
    fx.input.no_request();
}

#[test]
fn an_active_template_batch_refuses_replacement_and_preserves_a_blocked_input_frame() {
    let mut fx = TemplateFixture::new();
    fx.begin();
    let request = fx.request(0);
    let error = fx
        .input
        .run
        .context
        .begin_template_checks(inventory(), &mut fx.input.run.clock)
        .unwrap_err();
    assert!(error.failure.is_none());
    assert_eq!(error.inventory.digest(), inventory().digest());
    let frame = TurnFrame::carry_from(br#"{"text":"Read file."}"#, 42);
    let error = fx
        .input
        .run
        .context
        .begin_input(frame.clone(), &mut fx.input.run.clock)
        .unwrap_err();
    assert_eq!(error.frame, frame);
    assert!(error.failure.is_none());
    assert!(fx.input.run.context.input.snapshot().is_none());
    assert_eq!(
        fx.input
            .run
            .context
            .flow
            .standing_classifier()
            .unwrap()
            .outstanding(),
        Some(&request)
    );
    assert_eq!(
        fx.input
            .run
            .context
            .template_transcript()
            .unwrap()
            .accepted_count(),
        0
    );
    assert!(fx.input.run.context.failure().is_none());
    fx.input.no_request();
}

#[test]
fn missing_or_malformed_template_results_preserve_prior_records_and_freeze() {
    for missing in [false, true] {
        let mut fx = TemplateFixture::new();
        fx.begin();
        let first = fx.accept(0);
        let request = fx.request(1);
        if missing {
            // SAFETY: the borrowed peer descriptor remains live for this call.
            assert_eq!(
                unsafe {
                    nix::libc::shutdown(fx.input.peer.readiness().as_raw_fd(), nix::libc::SHUT_WR)
                },
                0
            );
        } else {
            fx.send_raw(
                request.request_id.0,
                br#"{"not":"a complete typed reply"}"#.to_vec(),
            );
        }
        fx.transport_result();
        let failure = fx.advance().unwrap_err();
        assert!(!matches!(failure, RunFailure::Wait { .. }));
        assert_eq!(fx.input.run.context.phase(), RecordingPhase::Failed);
        assert!(fx.input.run.context.halt().is_some());
        assert!(fx.certificate().is_err());
        let transcript = fx.input.run.context.template_transcript().unwrap();
        assert_eq!(transcript.accepted_count(), 1);
        assert_eq!(transcript.records().len(), 1);
        assert_eq!(transcript.records()[0].reply(), &first);
        assert_eq!(fx.input.records(Kind::Ste100Result).len(), 1);
        assert_eq!(fx.advance().unwrap_err(), failure);
    }
}

#[test]
fn a_component_failed_template_result_is_recorded_before_the_driver_freezes() {
    let mut fx = TemplateFixture::new();
    fx.begin();
    let request = fx.request(0);
    let mut reply = accepted(&request);
    reply.result = s::Ste100Result::ComponentFailed {
        code: s::Ste100ComponentCode::NoResult,
        detail: "fixture checker could not produce an inclusion result".into(),
    };
    fx.input.reply(&reply);
    assert!(fx.input.records(Kind::Ste100Result).is_empty());
    let mut failure = None;
    for _ in 0..128 {
        match fx.advance() {
            Ok(_) => {}
            Err(error) => {
                failure = Some(error);
                break;
            }
        }
    }
    assert!(failure.is_some());
    let records = fx.input.records(Kind::Ste100Result);
    assert_eq!(records.len(), 1);
    assert_eq!(records[0]["payload"], serde_json::to_value(&reply).unwrap());
    assert!(fx.input.run.context.halt().is_some());
    assert!(fx.certificate().is_err());
    assert_eq!(
        fx.input
            .run
            .context
            .template_transcript()
            .unwrap()
            .accepted_count(),
        0
    );
    assert_eq!(fx.input.peer.try_receive(), Err(Fault::Closed));
}

#[test]
fn the_generic_operation_consumer_cannot_steal_a_recorded_template_result() {
    let mut fx = TemplateFixture::new();
    fx.begin();
    let request = fx.request(0);
    let reply = accepted(&request);
    fx.input.reply(&reply);
    for _ in 0..128 {
        fx.input
            .run
            .context
            .reduce_flow(&mut fx.input.run.clock)
            .unwrap();
        if fx.input.run.context.flow.operation_completion().is_some() {
            break;
        }
    }
    assert_eq!(fx.input.records(Kind::Ste100Result).len(), 1);
    let Some(OperationCompletion::Check(recorded)) =
        fx.input.run.context.flow.operation_completion()
    else {
        panic!("the actual recorded check must stand in its completion slot");
    };
    assert_eq!(recorded.reply(), &reply);
    let source = recorded.source().clone();
    for _ in 0..3 {
        assert!(
            fx.input
                .run
                .context
                .take_operation_completion(&mut fx.input.run.clock)
                .unwrap()
                .is_none()
        );
        assert_eq!(
            fx.input
                .run
                .context
                .template_transcript()
                .unwrap()
                .accepted_count(),
            0
        );
        assert!(fx.input.run.context.flow.operation_completion().is_some());
    }
    fx.until_accepted(1);
    let transcript = fx.input.run.context.template_transcript().unwrap();
    assert_eq!(transcript.records()[0].reply(), &reply);
    assert_eq!(transcript.records()[0].result_source(), &source);
    assert!(fx.input.run.context.flow.operation_completion().is_none());
    assert_eq!(fx.input.records(Kind::Ste100Result).len(), 1);
}

#[test]
fn the_enter_wait_stays_pending_until_certification_finishes_or_refuses_load() {
    for reject in [false, true] {
        let mut fx = TemplateFixture::new();
        let original = fx
            .input
            .run
            .context
            .deadlines
            .get(PumpDeadline::Enter)
            .unwrap();
        fx.begin();
        let request = fx.request(0);
        assert!(
            fx.input
                .run
                .context
                .complete_wait(PumpDeadline::Enter)
                .is_none()
        );
        assert_eq!(
            fx.input.run.context.deadlines.get(PumpDeadline::Enter),
            Some(original)
        );
        let mut reply = accepted(&request);
        if reject {
            reply.result = s::Ste100Result::NotSte100 {
                code: s::Ste100DefectCode::NonSte100,
                detail: "fixture template has no inclusion proof".into(),
                spans: vec![],
            };
        }
        fx.input.reply(&reply);
        if reject {
            for _ in 0..128 {
                if fx.advance().unwrap() == TemplateProgress::Rejected {
                    break;
                }
            }
            assert_eq!(fx.advance().unwrap(), TemplateProgress::Rejected);
            assert!(fx.certificate().unwrap().is_none());
            assert_eq!(
                fx.input.run.context.complete_wait(PumpDeadline::Enter),
                Some(original)
            );
            fx.input
                .run
                .clock
                .advance_to(original.deadline_ns() + 1)
                .unwrap();
            let leave = AbsoluteBound::after(original.deadline_ns() + 1, 100).unwrap();
            fx.input
                .run
                .context
                .begin_releasing(leave, &mut fx.input.run.clock)
                .unwrap();
            assert_eq!(fx.input.run.context.phase(), RecordingPhase::Releasing);
            assert_eq!(
                fx.input.run.context.deadlines.get(PumpDeadline::Leave),
                Some(leave)
            );
            assert!(fx.certificate().is_err());
            assert_eq!(
                fx.input
                    .run
                    .context
                    .template_transcript()
                    .unwrap()
                    .rejection(),
                Some(&reply)
            );
            let frame = TurnFrame::carry_from(br#"{"text":"Read file."}"#, 42);
            let refused = fx
                .input
                .run
                .context
                .begin_input(frame.clone(), &mut fx.input.run.clock)
                .unwrap_err();
            assert_eq!(refused.frame, frame);
            assert!(refused.failure.is_none());
            fx.input.no_request();
        } else {
            fx.until_accepted(1);
            fx.accept(1);
            fx.accept(2);
            assert!(fx.certificate().unwrap().is_some());
            assert_eq!(
                fx.input.run.context.complete_wait(PumpDeadline::Enter),
                Some(original)
            );
        }
        assert!(fx.input.run.context.failure().is_none());
        assert!(fx.input.run.context.halt().is_none());
    }
}

#[test]
fn content_authorship_and_member_capture_are_blocked_without_halting_template_checks() {
    let mut fx = TemplateFixture::new();
    fx.begin();
    let request = fx.request(0);
    let authored = AuthoredEvent {
        kind: Kind::MessageAssistant,
        subsystem: weaver_trace::Subsystem::Harness,
        turn: Some(weaver_trace::TurnRef("blocked-turn".into())),
        source: super::super::author::AuthoredSource::Model,
        causal_parent: Some(Sequence(0)),
        payload: Some(Payload::Message(
            serde_json::value::to_raw_value(&serde_json::json!({
                "role": "assistant",
                "content": [{"type":"text", "text":"Read file."}]
            }))
            .unwrap(),
        )),
    };
    let original = authored.clone();
    assert_eq!(
        fx.input.run.author(authored).unwrap_err(),
        RunFailure::TemplateBlocked
    );
    assert!(fx.input.run.context.refused_author().is_none());
    let refused = fx.input.run.context.take_template_blocked_author().unwrap();
    assert_eq!(refused.kind, original.kind);
    assert_eq!(refused.subsystem, original.subsystem);
    assert_eq!(refused.turn, original.turn);
    assert_eq!(refused.causal_parent, original.causal_parent);
    assert!(matches!(
        refused.source,
        super::super::author::AuthoredSource::Model
    ));
    let (Some(Payload::Message(actual)), Some(Payload::Message(expected))) =
        (&refused.payload, &original.payload)
    else {
        panic!("blocked caller must retain the original message payload");
    };
    assert_eq!(actual.get(), expected.get());
    assert!(
        fx.input
            .run
            .context
            .take_template_blocked_author()
            .is_none()
    );
    assert!(fx.input.run.context.pending_author().is_none());
    assert!(fx.input.records(Kind::MessageAssistant).is_empty());
    let mut capture = TemplateFixture::new();
    capture.begin();
    assert!(capture.input.run.context.flow.pending_check().is_none());
    let now = capture.input.run.clock.now().unwrap();
    let save_wait = capture
        .input
        .run
        .context
        .load_duty
        .as_ref()
        .unwrap()
        .persistence
        .save_wait
        .elapsed_ns
        .0;
    let window = s::SaveWindow {
        began_ns: now.into(),
        deadline_ns: now.checked_add(save_wait).unwrap().into(),
    };
    window.validate().unwrap();
    assert_eq!(
        capture
            .input
            .run
            .context
            .prepare_member_capture(1.into(), window, &mut capture.input.run.clock)
            .unwrap_err(),
        RunFailure::TemplateBlocked,
    );
    assert!(capture.input.run.context.capture_snapshot().is_none());
    assert!(
        capture
            .input
            .run
            .context
            .deadlines
            .get(PumpDeadline::CaptureWindow)
            .is_none()
    );
    assert!(capture.input.run.context.failure().is_none());
    assert!(capture.input.run.context.halt().is_none());
    capture.input.no_request();
    assert!(fx.input.run.context.failure().is_none());
    assert!(fx.input.run.context.halt().is_none());
    assert_eq!(
        fx.input
            .run
            .context
            .flow
            .standing_classifier()
            .unwrap()
            .outstanding(),
        Some(&request)
    );
    fx.input.reply(&accepted(&request));
    fx.until_accepted(1);
    assert_eq!(fx.input.records(Kind::Ste100Result).len(), 1);
}

#[test]
fn a_second_blocked_author_preserves_both_original_events_and_halts_protocol_misuse() {
    let mut fx = TemplateFixture::new();
    fx.begin();
    let request = fx.request(0);
    let authored = |text: &str| AuthoredEvent {
        kind: Kind::MessageAssistant,
        subsystem: weaver_trace::Subsystem::Harness,
        turn: Some(weaver_trace::TurnRef("blocked-turn".into())),
        source: super::super::author::AuthoredSource::Model,
        causal_parent: Some(Sequence(0)),
        payload: Some(Payload::Message(
            serde_json::value::to_raw_value(&serde_json::json!({
                "role": "assistant", "content": [{"type":"text", "text":text}]
            }))
            .unwrap(),
        )),
    };
    let first = authored("Read first file.");
    let second = authored("Read second file.");
    assert_eq!(
        fx.input.run.author(first.clone()).unwrap_err(),
        RunFailure::TemplateBlocked
    );
    assert!(fx.input.run.context.halt().is_none());
    assert!(matches!(
        fx.input.run.author(second.clone()).unwrap_err(),
        RunFailure::Flow {
            reason: FlowHaltReason::ProtocolFailed,
            point: HaltPoint::Author,
            ..
        }
    ));
    assert_eq!(fx.input.run.context.phase(), RecordingPhase::Failed);
    let held_first = fx.input.run.context.take_template_blocked_author().unwrap();
    let held_second = fx.input.run.context.refused_author().unwrap();
    for (actual, expected) in [(&held_first, &first), (held_second, &second)] {
        assert_eq!(actual.kind, expected.kind);
        assert_eq!(actual.subsystem, expected.subsystem);
        assert_eq!(actual.turn, expected.turn);
        assert_eq!(actual.causal_parent, expected.causal_parent);
        assert!(matches!(
            actual.source,
            super::super::author::AuthoredSource::Model
        ));
        let (Some(Payload::Message(actual)), Some(Payload::Message(expected))) =
            (&actual.payload, &expected.payload)
        else {
            panic!("both blocked authors must retain their original payloads");
        };
        assert_eq!(actual.get(), expected.get());
    }
    assert!(
        fx.input
            .run
            .context
            .take_template_blocked_author()
            .is_none()
    );
    assert!(fx.input.run.context.pending_author().is_none());
    assert!(fx.input.records(Kind::MessageAssistant).is_empty());
    assert!(fx.input.records(Kind::Ste100Result).is_empty());
    assert_eq!(fx.input.run.context.budget.gauges(), (0, 0));
    assert_eq!(
        fx.input
            .run
            .context
            .flow
            .standing_classifier()
            .unwrap()
            .outstanding(),
        Some(&request)
    );
    assert!(fx.certificate().is_err());
}
