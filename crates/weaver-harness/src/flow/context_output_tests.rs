//! Whole generation checks through the real recorder and Unix socket owners.
//! Scripted checker and member replies exercise mechanics, not qualification.

use super::input_tests::InputFixture;
use super::*;
use crate::flow::author::AuthoredSource;
use crate::flow::coordinator::{OutputOriginal, TransportSource, tests::operation};
use crate::flow::ledger::LedgerState;
use crate::flow::pump::LifecyclePorts;
use crate::flow::ste100_channel::{Receive, Tag};
use std::io::{Read, Write};
use weaver_trace::{Subsystem, TurnRef};
use weaver_traits::{ContentBlock, Message, Role, ToolCall};
use weaver_types::Finish;
use weaver_types::state_flow as s;

fn ports<I: PumpIo>(io: &mut I) -> PumpPorts<'_, I> {
    PumpPorts {
        lifecycle: LifecyclePorts::default(),
        gate: None,
        decode: None,
        label: None,
        io,
    }
}

fn text(value: &str) -> ContentBlock {
    ContentBlock::Text { text: value.into() }
}

fn original(content: Vec<ContentBlock>) -> OutputOriginal {
    OutputOriginal {
        message: Message {
            role: Role::Assistant,
            content,
        },
        finish: Finish::Completed,
        turn: TurnRef("output-turn".into()),
    }
}

fn older_event(value: &str) -> AuthoredEvent {
    AuthoredEvent {
        kind: Kind::MessageAssistant,
        subsystem: Subsystem::Harness,
        turn: Some(TurnRef("output-turn".into())),
        source: AuthoredSource::Model,
        causal_parent: None,
        payload: Some(Payload::Message(
            serde_json::value::to_raw_value(&Message {
                role: Role::Assistant,
                content: vec![text(value)],
            })
            .unwrap(),
        )),
    }
}

fn reply(request: &s::Ste100Request, reject: bool) -> s::Ste100Reply {
    let result = if reject {
        s::Ste100Result::NotSte100 {
            code: s::Ste100DefectCode::NonSte100,
            detail: "scripted whole-output defect".into(),
            spans: Vec::new(),
        }
    } else {
        assert!(request.text.starts_with("Read ") && request.text.ends_with('.'));
        let length = request.text.len() as u64;
        s::Ste100Result::Accepted {
            bindings: vec![s::Binding {
                statement_span: s::Span {
                    start: 0.into(),
                    end: length.into(),
                },
                verb: "read".into(),
                verb_span: s::Span {
                    start: 0.into(),
                    end: 4.into(),
                },
                condition: None,
                condition_span: None,
                nouns: vec![s::NounBinding {
                    value: request.text[5..request.text.len() - 1].into(),
                    span: s::Span {
                        start: 5.into(),
                        end: (length - 1).into(),
                    },
                }],
            }],
        }
    };
    s::Ste100Reply {
        request_id: request.request_id,
        target: request.target.clone(),
        scheme_id: request.scheme_id.clone(),
        result,
    }
}

struct OutputFixture {
    input: InputFixture,
    bound: AbsoluteBound,
    requests: Vec<s::Ste100Request>,
    received: Vec<u8>,
    file_requests: Vec<s::StateFrame>,
    operation: u64,
}

impl OutputFixture {
    fn new() -> Self {
        Self::configured(super::tests::config())
    }

    fn configured(declaration: FlowConfig) -> Self {
        let mut input = InputFixture::with_declaration(None, declaration);
        input.run.state.set_nonblocking(true).unwrap();
        input
            .run
            .author(AuthoredEvent {
                kind: Kind::TurnStarted,
                subsystem: Subsystem::Harness,
                turn: Some(TurnRef("output-turn".into())),
                source: AuthoredSource::Runtime,
                causal_parent: None,
                payload: None,
            })
            .unwrap();
        let bound = AbsoluteBound::after(104, 1_000_000).unwrap();
        input
            .run
            .context
            .arm(PumpDeadline::Decision, bound, &mut input.run.clock)
            .unwrap();
        Self {
            input,
            bound,
            requests: Vec::new(),
            received: Vec::new(),
            file_requests: Vec::new(),
            operation: 2,
        }
    }

    fn begin(&mut self, original: OutputOriginal) {
        self.input
            .run
            .context
            .begin_output(original, Some(Sequence(0)), &mut self.input.run.clock)
            .unwrap();
    }

    fn advance(&mut self) -> Result<OutputProgress, RunFailure> {
        self.input
            .run
            .context
            .advance_output(&mut ports(&mut self.input.run.clock))
    }

    fn transport(&mut self, source: TransportSource) {
        self.input
            .run
            .context
            .flow
            .transport_one(
                &mut FlowRecording {
                    barrier: &mut self.input.run.context.barrier,
                    inbox: &mut self.input.run.context.inbox,
                    budget: &mut self.input.run.context.budget,
                    author: &self.input.run.context.author,
                },
                &mut self.input.run.clock,
                source,
            )
            .unwrap();
    }

    fn receive_check(&mut self) -> Option<s::Ste100Request> {
        self.input.classifier_transport();
        let Receive::Complete(message) = self.input.peer.try_receive().unwrap() else {
            return None;
        };
        assert_eq!(message.tag, Tag::Request);
        let request: s::Ste100Request =
            s::decode_strict(std::str::from_utf8(&message.payload).unwrap()).unwrap();
        assert_eq!(message.id, request.request_id.0);
        self.requests.push(request.clone());
        Some(request)
    }

    fn member(&mut self, answer: bool) {
        self.transport(TransportSource::State);
        let mut bytes = [0; 65536];
        loop {
            match self.input.run.state.read(&mut bytes) {
                Ok(0) => panic!("member socket closed"),
                Ok(n) => self.received.extend_from_slice(&bytes[..n]),
                Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => break,
                Err(error) => panic!("member fixture read: {error}"),
            }
        }
        while let Some(end) = self.received.iter().position(|byte| *byte == b'\n') {
            let line: Vec<_> = self.received.drain(..=end).collect();
            let request = s::decode_state_line(&line, s::FrameDirection::Request).unwrap();
            let s::StateFrame::File { filing, .. } = &request else {
                panic!("output fixture saw nonstatement filing");
            };
            if answer {
                let response = s::StateFrame::Filed {
                    common: request.common().clone(),
                    block_id: filing.block_id.clone(),
                    observation: operation(&request, self.operation, 104 + self.operation * 2),
                };
                self.operation += 1;
                self.input
                    .run
                    .clock
                    .advance_to(104 + self.operation * 2)
                    .unwrap();
                self.input
                    .run
                    .state
                    .write_all(
                        &s::encode_state_line(&response, s::FrameDirection::Response).unwrap(),
                    )
                    .unwrap();
            }
            self.file_requests.push(request);
            self.transport(TransportSource::State);
        }
    }

    fn step(&mut self, reject: Option<&str>, file: bool) -> OutputProgress {
        let progress = self.advance().unwrap();
        if let Some(request) = self.receive_check() {
            self.input
                .reply(&reply(&request, reject == Some(request.text.as_str())));
        }
        self.member(file);
        progress
    }

    fn complete(&mut self, reject: Option<&str>, file: bool) {
        for _ in 0..256 {
            if self.step(reject, file) == OutputProgress::Complete {
                return;
            }
        }
        panic!("whole output did not reach its recorded check frontier");
    }

    fn request(&mut self) -> s::Ste100Request {
        for _ in 0..128 {
            self.advance().unwrap();
            if let Some(request) = self.receive_check() {
                return request;
            }
            self.member(false);
        }
        panic!("whole output did not submit its check");
    }

    fn drive_to_file(&mut self) {
        for _ in 0..128 {
            self.input
                .run
                .context
                .reduce_flow(&mut self.input.run.clock)
                .unwrap();
            self.member(false);
            if !self.file_requests.is_empty() {
                self.input
                    .run
                    .context
                    .reduce_flow(&mut self.input.run.clock)
                    .unwrap();
                return;
            }
        }
        panic!("accepted ordinary filing never reached the member");
    }
}

#[test]
fn mixed_blocks_keep_original_indices_and_finish_checks_without_a_filing_receipt() {
    let mut fx = OutputFixture::new();
    let content = vec![
        text("Read first."),
        ContentBlock::ToolCall(ToolCall {
            name: "fixture".into(),
            arguments: "Read requested.".into(),
        }),
        text("Read selected."),
        ContentBlock::UnrecoveredCall {
            fragment: "<fixture unfinished call>".into(),
        },
        text("Read last."),
    ];
    fx.begin(original(content.clone()));
    fx.complete(None, false);
    let checked = fx
        .input
        .run
        .context
        .take_output(&mut fx.input.run.clock)
        .unwrap()
        .unwrap();
    assert_eq!(checked.original().message.content, content);
    assert_eq!(checked.original().finish, Finish::Completed);
    assert_eq!(checked.source().block_id.run, "run");
    let sequence = checked.source().block_id.sequence;
    let recorded = fx.input.records(Kind::MessageAssistant);
    assert_eq!(recorded.len(), 1);
    assert_eq!(recorded[0]["sequence"], serde_json::json!(sequence));
    let indices: Vec<_> = checked
        .blocks()
        .iter()
        .map(|block| match &block.request().target {
            s::Ste100Target::Source(source) => {
                assert_eq!(source.sequence, sequence);
                assert_eq!(source.run, "run");
                source.block.0
            }
            _ => panic!("model checks must name actual recorded source blocks"),
        })
        .collect();
    assert_eq!(indices, vec![0, 2, 4]);
    assert_eq!(
        checked
            .blocks()
            .iter()
            .map(|b| b.selected())
            .collect::<Vec<_>>(),
        vec![true, true, true]
    );
    assert_eq!(
        fx.requests.len(),
        3,
        "selected Text must not be checked twice"
    );
    fx.drive_to_file();
    assert_eq!(fx.file_requests.len(), 1);
    let s::StateFrame::File { filing, .. } = &fx.file_requests[0] else {
        unreachable!();
    };
    assert_eq!(filing.block_id.block.0, 0);
    assert_eq!(filing.text, "Read first.");
    assert!(fx.input.run.context.flow.ledger().facts().any(|entry| {
        entry.source == &filing.block_id && entry.state == LedgerState::AwaitingReceipt
    }));
    assert_eq!(fx.input.records(Kind::Ste100Result).len(), 3);
    assert!(fx.input.run.context.failure().is_none());
    assert_eq!(
        fx.input.run.context.deadlines.get(PumpDeadline::Decision),
        Some(fx.bound)
    );
    assert!(
        fx.input
            .run
            .context
            .take_output(&mut fx.input.run.clock)
            .unwrap()
            .is_none()
    );
}

#[test]
fn a_last_block_rejection_holds_the_complete_generation_until_its_recorded_disposition() {
    let mut fx = OutputFixture::new();
    fx.begin(original(vec![
        text("Read accepted."),
        text("Read rejected."),
    ]));
    let first = fx.request();
    assert_eq!(first.text, "Read accepted.");
    fx.input.reply(&reply(&first, false));
    let last = fx.request();
    assert_eq!(last.text, "Read rejected.");
    assert!(
        fx.input
            .run
            .context
            .take_output(&mut fx.input.run.clock)
            .unwrap()
            .is_none()
    );
    assert_eq!(fx.input.records(Kind::Ste100Result).len(), 1);
    fx.input.reply(&reply(&last, true));
    assert!(
        fx.input
            .run
            .context
            .take_output(&mut fx.input.run.clock)
            .unwrap()
            .is_none()
    );
    fx.complete(None, false);
    let checked = fx
        .input
        .run
        .context
        .take_output(&mut fx.input.run.clock)
        .unwrap()
        .unwrap();
    assert!(matches!(
        checked.blocks()[0].reply().result,
        s::Ste100Result::Accepted { .. }
    ));
    assert!(matches!(
        checked.blocks()[1].reply().result,
        s::Ste100Result::NotSte100 { .. }
    ));
    assert_eq!(fx.requests.len(), 2);
    assert!(fx.input.run.context.failure().is_none());
}

#[test]
fn checker_reply_does_not_complete_before_the_actual_result_trace_record() {
    let mut fx = OutputFixture::new();
    fx.begin(original(vec![text("Read held.")]));
    let request = fx.request();
    fx.input.fill_trace();
    fx.input.reply(&reply(&request, false));
    for _ in 0..16 {
        fx.advance().unwrap();
        if fx.input.run.context.flow.has_pending_record() {
            break;
        }
    }
    assert!(fx.input.run.context.flow.has_pending_record());
    assert!(fx.input.records(Kind::Ste100Result).is_empty());
    assert!(
        fx.input
            .run
            .context
            .take_output(&mut fx.input.run.clock)
            .unwrap()
            .is_none()
    );
    fx.input.drain_trace();
    fx.complete(None, false);
    let checked = fx
        .input
        .run
        .context
        .take_output(&mut fx.input.run.clock)
        .unwrap()
        .unwrap();
    assert_eq!(fx.input.records(Kind::Ste100Result).len(), 1);
    let result_source = checked.blocks()[0].result_source();
    assert!(result_source.block_id.sequence > checked.source().block_id.sequence);
}

#[test]
fn no_text_generation_is_still_recorded_and_retains_attempted_calls() {
    let mut fx = OutputFixture::new();
    let content = vec![ContentBlock::UnrecoveredCall {
        fragment: "<broken>".into(),
    }];
    let mut output = original(content.clone());
    output.finish = Finish::Length;
    fx.begin(output);
    fx.complete(None, false);
    let checked = fx
        .input
        .run
        .context
        .take_output(&mut fx.input.run.clock)
        .unwrap()
        .unwrap();
    assert_eq!(checked.original().message.content, content);
    assert_eq!(checked.original().finish, Finish::Length);
    assert!(checked.blocks().is_empty());
    assert!(fx.requests.is_empty());
    assert!(fx.file_requests.is_empty());
    assert_eq!(fx.input.records(Kind::MessageAssistant).len(), 1);
    assert!(checked.source().block_id.sequence.0 > 0);
}

#[test]
fn cancellation_before_authoring_returns_unchanged_original_without_a_source_or_grant() {
    let mut fx = OutputFixture::new();
    let output = original(vec![text("Read cancellation.")]);
    let message = output.message.clone();
    fx.begin(output);
    fx.input.run.context.cancel_output();
    fx.input.run.context.cancel_output();
    let cancellation = fx.input.run.context.take_cancelled_output().unwrap();
    assert_eq!(cancellation.original.message, message);
    assert!(cancellation.source.is_none());
    assert!(cancellation.recorded.is_empty());
    assert!(fx.input.records(Kind::MessageAssistant).is_empty());
    assert!(fx.input.run.context.flow.ledger().facts().next().is_none());
    assert!(
        fx.input
            .run
            .context
            .take_output(&mut fx.input.run.clock)
            .unwrap()
            .is_none()
    );
    assert_eq!(
        fx.input.run.context.deadlines.get(PumpDeadline::Decision),
        Some(fx.bound)
    );
}

#[test]
fn selected_check_survives_cancelled_generation_as_an_ordinary_filing_obligation() {
    let mut fx = OutputFixture::new();
    fx.begin(original(vec![text("Read cancellation.")]));
    let request = fx.request();
    fx.input.run.context.cancel_output();
    let cancellation = fx.input.run.context.take_cancelled_output().unwrap();
    assert!(cancellation.source.is_some());
    assert!(cancellation.recorded.is_empty());
    assert!(
        fx.input
            .run
            .context
            .take_output(&mut fx.input.run.clock)
            .unwrap()
            .is_none()
    );
    fx.input
        .run
        .context
        .complete_wait(PumpDeadline::Decision)
        .unwrap();
    assert!(fx.input.run.context.flow.ledger().facts().next().is_some());
    fx.input.reply(&reply(&request, false));
    for _ in 0..128 {
        fx.input
            .run
            .context
            .reduce_flow(&mut fx.input.run.clock)
            .unwrap();
        fx.member(true);
        if fx.input.run.context.flow.ledger().facts().next().is_none() {
            break;
        }
    }
    assert_eq!(fx.file_requests.len(), 1);
    assert!(fx.input.run.context.flow.ledger().facts().next().is_none());
    assert_eq!(fx.input.records(Kind::Ste100Result).len(), 1);
    assert_eq!(fx.input.records(Kind::MessageAssistant).len(), 1);
    assert!(fx.input.run.context.failure().is_none());
}

#[test]
fn pending_checks_refuse_wait_retirement_and_expire_the_original_decision_bound() {
    let mut fx = OutputFixture::new();
    fx.begin(original(vec![text("Read first."), text("Read delayed.")]));
    let first = fx.request();
    fx.input
        .run
        .clock
        .advance_to(fx.bound.deadline_ns() - 1)
        .unwrap();
    fx.input.reply(&reply(&first, false));
    let second = fx.request();
    assert_eq!(second.text, "Read delayed.");
    assert!(
        fx.input
            .run
            .context
            .complete_wait(PumpDeadline::Decision)
            .is_none()
    );
    assert_eq!(
        fx.input.run.context.deadlines.get(PumpDeadline::Decision),
        Some(fx.bound)
    );
    fx.input
        .run
        .clock
        .advance_to(fx.bound.deadline_ns())
        .unwrap();
    assert!(matches!(
        fx.advance(),
        Err(RunFailure::Flow {
            reason: FlowHaltReason::DecisionBound,
            ..
        })
    ));
    assert!(
        fx.input
            .run
            .context
            .take_output(&mut fx.input.run.clock)
            .is_err()
    );
    assert_eq!(fx.input.records(Kind::Ste100Result).len(), 1);
    assert!(fx.input.run.context.flow.ledger().facts().any(|entry| {
        entry.source.block.0 == 1 && entry.state == LedgerState::AwaitingClassification
    }));
}

#[test]
fn timely_recorded_checks_can_be_taken_after_the_borrowed_wait_is_retired() {
    let mut fx = OutputFixture::new();
    fx.begin(original(vec![text("Read completed.")]));
    let request = fx.request();
    fx.input.run.clock.advance_to(200).unwrap();
    fx.input.reply(&reply(&request, false));
    fx.complete(None, false);
    fx.input
        .run
        .context
        .complete_wait(PumpDeadline::Decision)
        .unwrap();
    fx.input
        .run
        .clock
        .advance_to(fx.bound.deadline_ns() + 1)
        .unwrap();
    let checked = fx
        .input
        .run
        .context
        .take_output(&mut fx.input.run.clock)
        .unwrap()
        .unwrap();
    assert_eq!(checked.recorded_ns(), 200);
    assert_eq!(checked.blocks()[0].recorded_ns(), 200);
    assert!(fx.input.run.context.failure().is_none());
}

#[test]
fn malformed_or_foreign_generation_is_returned_before_any_author_or_check() {
    for case in 0..3 {
        let mut fx = OutputFixture::new();
        let mut output = original(vec![text("Read refused.")]);
        match case {
            0 => output.message.role = Role::User,
            1 => output.turn = TurnRef("another-turn".into()),
            _ => {
                output.message.content =
                    vec![ContentBlock::ToolResult(weaver_traits::ToolResultBlock {
                        content: "Read illegal block.".into(),
                    })]
            }
        }
        let expected = format!("{output:?}");
        let error = fx
            .input
            .run
            .context
            .begin_output(output, None, &mut fx.input.run.clock)
            .unwrap_err();
        assert_eq!(format!("{:?}", error.original), expected);
        assert!(error.failure.is_none());
        assert!(fx.input.records(Kind::MessageAssistant).is_empty());
        assert!(fx.input.run.context.flow.ledger().facts().next().is_none());
        fx.input.no_request();
    }
}

#[test]
fn output_owner_excludes_another_input_and_filing_consumer_until_taken() {
    let mut fx = OutputFixture::new();
    fx.begin(original(vec![text("Read held owner.")]));
    fx.complete(None, false);
    let frame = weaver_types::TurnFrame::carry_from(br#"{"text":"Read next."}"#, 42);
    let refused = fx
        .input
        .run
        .context
        .begin_input(frame.clone(), &mut fx.input.run.clock)
        .unwrap_err();
    assert_eq!(refused.frame, frame);
    assert!(refused.failure.is_none());
    let event = AuthoredEvent {
        kind: Kind::MessageAssistant,
        subsystem: Subsystem::Harness,
        turn: Some(TurnRef("output-turn".into())),
        source: AuthoredSource::Model,
        causal_parent: None,
        payload: Some(Payload::Message(
            serde_json::value::to_raw_value(&Message {
                role: Role::Assistant,
                content: vec![text("Read competitor.")],
            })
            .unwrap(),
        )),
    };
    let expected = format!("{event:?}");
    let error = fx
        .input
        .run
        .context
        .begin_filing(event, PumpDeadline::Decision, &mut fx.input.run.clock)
        .unwrap_err();
    assert_eq!(format!("{:?}", error.event), expected);
    assert!(error.failure.is_none());
    assert_eq!(fx.input.records(Kind::MessageAssistant).len(), 1);
    assert!(
        fx.input
            .run
            .context
            .take_output(&mut fx.input.run.clock)
            .unwrap()
            .is_some()
    );
}

#[test]
fn a_taken_check_frontier_allows_a_later_generation_while_ordinary_filing_waits() {
    let mut fx = OutputFixture::new();
    fx.begin(original(vec![text("Read first.")]));
    fx.complete(None, false);
    let first = fx
        .input
        .run
        .context
        .take_output(&mut fx.input.run.clock)
        .unwrap()
        .unwrap();
    fx.drive_to_file();
    assert_eq!(fx.file_requests.len(), 1);
    assert!(
        fx.input
            .run
            .context
            .flow
            .ledger()
            .facts()
            .any(|entry| entry.state == LedgerState::AwaitingReceipt)
    );
    fx.begin(original(vec![text("Read second.")]));
    fx.complete(None, false);
    let second = fx
        .input
        .run
        .context
        .take_output(&mut fx.input.run.clock)
        .unwrap()
        .unwrap();
    assert!(second.source().block_id.sequence > first.source().block_id.sequence);
    assert_eq!(second.blocks()[0].request().text, "Read second.");
    assert_eq!(fx.requests.len(), 2);
    assert_eq!(
        fx.file_requests.len(),
        1,
        "the first member exchange still owns the filing channel"
    );
    assert_eq!(fx.input.run.context.flow.ledger().facts().count(), 2);
    assert!(fx.input.run.context.failure().is_none());
}

#[test]
fn a_cancelled_pending_author_keeps_its_ticket_and_selected_obligations() {
    let mut fx = OutputFixture::new();
    fx.input.fill_trace();
    fx.input
        .run
        .context
        .arm(
            PumpDeadline::Label,
            AbsoluteBound::after(104, 1).unwrap(),
            &mut fx.input.run.clock,
        )
        .unwrap();
    fx.begin(original(vec![text("Read pending author.")]));
    assert!(matches!(
        fx.advance(),
        Err(RunFailure::Wait {
            key: PumpDeadline::Label,
            ..
        })
    ));
    let ticket = fx.input.run.context.pending_author().unwrap().ticket;
    fx.input.run.context.cancel_output();
    assert!(fx.input.run.context.take_cancelled_output().is_none());
    assert_eq!(
        fx.input.run.context.pending_author().unwrap().ticket,
        ticket
    );
    fx.input
        .run
        .context
        .complete_wait(PumpDeadline::Label)
        .unwrap();
    fx.input.run.context.complete_wait(PumpDeadline::Decision);
    fx.input.drain_trace();
    for _ in 0..64 {
        if fx.advance().unwrap() == OutputProgress::Cancelled {
            break;
        }
    }
    let cancellation = fx.input.run.context.take_cancelled_output().unwrap();
    assert!(cancellation.source.is_some());
    assert!(cancellation.recorded.is_empty());
    assert_eq!(fx.input.records(Kind::MessageAssistant).len(), 1);
    assert!(fx.input.run.context.flow.ledger().facts().next().is_some());
    assert!(fx.input.run.context.failure().is_none());
}

#[test]
fn generic_resume_cannot_take_the_pending_output_author() {
    let mut fx = OutputFixture::new();
    fx.input.fill_trace();
    fx.input
        .run
        .context
        .arm(
            PumpDeadline::Label,
            AbsoluteBound::after(104, 1).unwrap(),
            &mut fx.input.run.clock,
        )
        .unwrap();
    fx.begin(original(vec![text("Read private author.")]));
    assert!(matches!(
        fx.advance(),
        Err(RunFailure::Wait {
            key: PumpDeadline::Label,
            ..
        })
    ));
    let ticket = fx.input.run.context.pending_author().unwrap().ticket;
    fx.input
        .run
        .context
        .complete_wait(PumpDeadline::Label)
        .unwrap();
    assert!(matches!(
        fx.input
            .run
            .context
            .resume_author_and_wait(&mut ports(&mut fx.input.run.clock)),
        Err(RunFailure::Flow {
            reason: FlowHaltReason::ProtocolFailed,
            ..
        })
    ));
    assert_eq!(
        fx.input.run.context.pending_author().unwrap().ticket,
        ticket
    );
    assert!(fx.input.records(Kind::MessageAssistant).is_empty());
}

#[test]
fn trace_pressure_uses_the_same_decision_deadline_without_renewal() {
    let mut fx = OutputFixture::new();
    fx.input.fill_trace();
    fx.begin(original(vec![text("Read trace pressure.")]));
    assert!(matches!(
        fx.advance(),
        Err(RunFailure::Flow {
            reason: FlowHaltReason::DecisionBound,
            ..
        })
    ));
    assert!(fx.input.run.context.pending_author().is_some());
    assert_eq!(
        fx.input.run.context.deadlines.get(PumpDeadline::Decision),
        Some(fx.bound)
    );
    assert!(
        fx.input
            .run
            .context
            .take_output(&mut fx.input.run.clock)
            .is_err()
    );
    assert!(fx.input.records(Kind::MessageAssistant).is_empty());
    assert!(fx.requests.is_empty());
}

#[test]
fn deferred_copy_binds_the_real_output_before_any_selected_result_retires() {
    let mut declaration = super::tests::config();
    declaration.backlog.max_pending_blocks = 1.into();
    let mut fx = OutputFixture::configured(declaration);
    let old = fx.input.run.author(older_event("Read older.")).unwrap();
    fx.begin(original(vec![text("Read watched.")]));
    assert_eq!(fx.advance().unwrap(), OutputProgress::Progress);
    assert!(fx.input.run.context.budget.waiting_copy().is_some());
    fx.complete(None, true);
    let checked = fx
        .input
        .run
        .context
        .take_output(&mut fx.input.run.clock)
        .unwrap()
        .unwrap();
    assert!(checked.source().block_id.sequence.0 > old.0);
    assert_eq!(checked.blocks().len(), 1);
    assert!(checked.blocks()[0].selected());
    assert_eq!(checked.blocks()[0].request().text, "Read watched.");
    assert_eq!(fx.requests.len(), 2);
    assert!(fx.input.run.context.budget.waiting_copy().is_none());
}

#[test]
fn queue_delay_behind_an_older_selected_check_is_inside_the_original_decision_bound() {
    let mut fx = OutputFixture::new();
    fx.input.run.author(older_event("Read older.")).unwrap();
    let mut older = None;
    for _ in 0..128 {
        fx.input
            .run
            .context
            .reduce_flow(&mut fx.input.run.clock)
            .unwrap();
        if let Some(request) = fx.receive_check() {
            older = Some(request);
            break;
        }
    }
    assert_eq!(older.unwrap().text, "Read older.");
    fx.begin(original(vec![text("Read queued.")]));
    assert_eq!(fx.advance().unwrap(), OutputProgress::Progress);
    for _ in 0..16 {
        assert_ne!(fx.advance().unwrap(), OutputProgress::Complete);
    }
    assert_eq!(fx.requests.len(), 1);
    fx.input
        .run
        .clock
        .advance_to(fx.bound.deadline_ns())
        .unwrap();
    assert!(matches!(
        fx.advance(),
        Err(RunFailure::Flow {
            reason: FlowHaltReason::DecisionBound,
            ..
        })
    ));
    assert_eq!(
        fx.input.run.context.deadlines.get(PumpDeadline::Decision),
        Some(fx.bound)
    );
    assert_eq!(fx.input.run.context.flow.ledger().facts().count(), 2);
    assert!(fx.input.records(Kind::Ste100Result).is_empty());
}

struct ResultBoundaryClock {
    before_reads: usize,
    after: u64,
}

impl PumpIo for ResultBoundaryClock {
    fn now(&mut self) -> std::io::Result<u64> {
        if self.before_reads == 0 {
            Ok(self.after)
        } else {
            self.before_reads -= 1;
            Ok(104)
        }
    }

    fn wait(
        &mut self,
        _: &[crate::flow::clock::WaitInterest<'_>],
        _: Option<u64>,
    ) -> std::io::Result<crate::flow::clock::WaitOutcome> {
        panic!("record-boundary clock must not wait");
    }
}

#[test]
fn final_check_recorded_at_the_decision_bound_cannot_mint_a_complete_generation() {
    let mut fx = OutputFixture::new();
    fx.begin(original(vec![text("Read boundary.")]));
    let request = fx.request();
    // Drain prior measurements before crossing the held classifier result.
    for _ in 0..16 {
        fx.advance().unwrap();
    }
    fx.input.reply(&reply(&request, false));
    // live, advance and author clock reads precede the result's Recorded sample.
    let mut clock = ResultBoundaryClock {
        before_reads: 3,
        after: fx.bound.deadline_ns(),
    };
    assert!(matches!(
        fx.input.run.context.reduce_flow(&mut clock),
        Err(RunFailure::Flow {
            reason: FlowHaltReason::DecisionBound,
            ..
        })
    ));
    assert_eq!(
        fx.input.records(Kind::Ste100Result).len(),
        1,
        "the fixture must cross Recorded rather than expire before authoring"
    );
    assert!(fx.input.run.context.take_output(&mut clock).is_err());
    assert_eq!(fx.input.records(Kind::MessageAssistant).len(), 1);
}
