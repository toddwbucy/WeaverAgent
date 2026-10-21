//! Actual recorder admission, STE100 packets and member framing feed the
//! private receipt consumer. Fixture replies do not qualify a production model.

use super::input_tests::InputFixture;
use super::*;
use crate::flow::author::AuthoredSource;
use crate::flow::coordinator::{TransportSource, tests::operation};
use crate::flow::ledger::{LedgerFile, LedgerState, SourceDisposition};
use crate::flow::pump::LifecyclePorts;
use crate::flow::ste100_channel::{Receive, Tag};
use std::io::{Read, Write};
use weaver_traits::{ContentBlock, Message, Role};
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

fn event(texts: &[&str]) -> AuthoredEvent {
    AuthoredEvent {
        kind: Kind::MessageAssistant,
        subsystem: weaver_trace::Subsystem::Harness,
        turn: Some(weaver_trace::TurnRef("receipt-turn".into())),
        source: AuthoredSource::Model,
        causal_parent: Some(Sequence(0)),
        payload: Some(Payload::Message(
            serde_json::value::to_raw_value(&Message {
                role: Role::Assistant,
                content: texts
                    .iter()
                    .map(|text| ContentBlock::Text {
                        text: (*text).into(),
                    })
                    .collect(),
            })
            .unwrap(),
        )),
    }
}

struct ReceiptFixture {
    input: InputFixture,
    bound: AbsoluteBound,
    received: Vec<u8>,
    operation: u64,
    file_requests: Vec<s::StateFrame>,
}

impl ReceiptFixture {
    fn new(limit: u64) -> Self {
        let mut input = InputFixture::with_pending_limit(limit);
        input.run.state.set_nonblocking(true).unwrap();
        let bound = AbsoluteBound::after(104, 1_000_000).unwrap();
        input
            .run
            .context
            .arm(PumpDeadline::Decision, bound, &mut input.run.clock)
            .unwrap();
        Self {
            input,
            bound,
            received: Vec::new(),
            operation: 2,
            file_requests: Vec::new(),
        }
    }

    fn begin(&mut self, original: AuthoredEvent) {
        self.input
            .run
            .context
            .begin_filing(original, PumpDeadline::Decision, &mut self.input.run.clock)
            .unwrap();
    }

    fn advance(&mut self) -> Result<FilingProgress, RunFailure> {
        self.input
            .run
            .context
            .advance_filing(&mut ports(&mut self.input.run.clock))
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

    fn classifier_reply(&mut self, reject: Option<&str>) {
        self.transport(TransportSource::Classifier);
        let Receive::Complete(message) = self.input.peer.try_receive().unwrap() else {
            return;
        };
        assert_eq!(message.tag, Tag::Request);
        let request: s::Ste100Request =
            s::decode_strict(std::str::from_utf8(&message.payload).unwrap()).unwrap();
        let len = request.text.len() as u64;
        let result = if reject == Some(request.text.as_str()) {
            s::Ste100Result::NotSte100 {
                code: s::Ste100DefectCode::NonSte100,
                detail: "fixture rejection".into(),
                spans: Vec::new(),
            }
        } else {
            s::Ste100Result::Accepted {
                bindings: vec![s::Binding {
                    statement_span: s::Span {
                        start: 0.into(),
                        end: len.into(),
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
                            end: (len - 1).into(),
                        },
                    }],
                }],
            }
        };
        self.input.reply(&s::Ste100Reply {
            request_id: request.request_id,
            target: request.target,
            scheme_id: request.scheme_id,
            result,
        });
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
            let block_id = match &request {
                s::StateFrame::File { filing, .. } => filing.block_id.clone(),
                s::StateFrame::FilePair { filing, .. } => filing.block_id.clone(),
                _ => panic!("receipt fixture saw nonfiling request"),
            };
            let response = s::StateFrame::Filed {
                common: request.common().clone(),
                block_id,
                observation: operation(&request, self.operation, 104 + self.operation * 2),
            };
            self.operation += 1;
            self.input
                .run
                .clock
                .advance_to(104 + self.operation * 2)
                .unwrap();
            if answer {
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

    fn step(&mut self, reject: Option<&str>) -> FilingProgress {
        let progress = self.advance().unwrap();
        self.classifier_reply(reject);
        self.member(true);
        progress
    }

    fn complete(&mut self, reject: Option<&str>) -> FilingCompletion {
        for _ in 0..256 {
            if self.step(reject) == FilingProgress::Complete {
                return self
                    .input
                    .run
                    .context
                    .take_filing(&mut self.input.run.clock)
                    .unwrap()
                    .unwrap();
            }
        }
        panic!("receipt transaction did not settle");
    }
}

#[test]
fn exact_recorded_sources_distinguish_commits_from_correct_rejections() {
    let mut fx = ReceiptFixture::new(100);
    fx.begin(event(&["Read alpha.", "Read rejected."]));
    assert_eq!(fx.advance().unwrap(), FilingProgress::Progress);
    let sequence = fx.input.run.context.last_author_sequence.unwrap();
    assert!(
        sequence.0 > 0,
        "the real recorder assigned the source after load"
    );
    assert!(
        fx.input
            .run
            .context
            .take_filing(&mut fx.input.run.clock)
            .unwrap()
            .is_none()
    );
    let complete = fx.complete(Some("Read rejected."));
    assert_eq!(complete.sequence(), sequence);
    assert_eq!(
        complete
            .receipts()
            .sources()
            .map(|source| source.block.0)
            .collect::<Vec<_>>(),
        vec![0, 1]
    );
    assert_eq!(
        fx.file_requests.len(),
        1,
        "correct rejection never entered the member"
    );
    let outcomes: Vec<_> = complete.receipts().dispositions().collect();
    let SourceDisposition::Committed(committed) = outcomes[0] else {
        panic!("first source committed");
    };
    let LedgerFile::Statement(file) = committed.file() else {
        panic!("statement");
    };
    assert_eq!(file.text, "Read alpha.");
    assert_eq!(file.block_id.sequence.0, sequence.0);
    assert_eq!(committed.receipt().common(), fx.file_requests[0].common());
    assert_eq!(
        committed.common().request_id,
        fx.file_requests[0].common().request_id
    );
    let SourceDisposition::CorrectlyRejected(rejected) = outcomes[1] else {
        panic!("second source rejected");
    };
    assert_eq!(rejected.file().text, "Read rejected.");
    assert!(matches!(
        rejected.reply().result,
        s::Ste100Result::NotSte100 { .. }
    ));
    assert_eq!(fx.input.run.context.budget.gauges(), (0, 0));
    assert_eq!(
        fx.input.run.context.deadlines.get(PumpDeadline::Decision),
        Some(fx.bound)
    );
    assert!(
        fx.input
            .run
            .context
            .take_filing(&mut fx.input.run.clock)
            .unwrap()
            .is_none()
    );
    assert!(fx.input.run.context.failure().is_none());
}

#[test]
fn deferred_whole_copy_registers_before_its_first_disposition_without_collecting_older_sources() {
    let mut fx = ReceiptFixture::new(1);
    let old = fx.input.run.author(event(&["Read older."])).unwrap();
    fx.begin(event(&["Read watched."]));
    assert_eq!(fx.advance().unwrap(), FilingProgress::Progress);
    let watched = fx.input.run.context.last_author_sequence.unwrap();
    assert!(watched.0 > old.0);
    assert!(fx.input.run.context.budget.waiting_copy().is_some());
    assert!(fx.input.run.context.flow.filing_receipts().is_none());
    let complete = fx.complete(None);
    assert_eq!(complete.receipts().sources().count(), 1);
    assert_eq!(
        complete.receipts().sources().next().unwrap().sequence.0,
        watched.0
    );
    assert_eq!(fx.file_requests.len(), 2);
    assert!(fx.input.run.context.budget.waiting_copy().is_none());
}

#[test]
fn member_receipt_does_not_release_consumer_before_the_terminal_trace_record() {
    let mut fx = ReceiptFixture::new(100);
    fx.begin(event(&["Read held."]));
    for _ in 0..128 {
        fx.step(None);
        if fx
            .input
            .run
            .context
            .flow
            .ledger()
            .facts()
            .any(|fact| fact.state == LedgerState::FiledHeld)
        {
            break;
        }
    }
    assert!(
        fx.input
            .run
            .context
            .flow
            .ledger()
            .facts()
            .any(|fact| fact.state == LedgerState::FiledHeld)
    );
    fx.input.fill_trace();
    assert_eq!(fx.advance().unwrap(), FilingProgress::Pending);
    assert!(fx.input.run.context.flow.has_pending_record());
    assert!(
        fx.input
            .run
            .context
            .take_filing(&mut fx.input.run.clock)
            .unwrap()
            .is_none()
    );
    assert_eq!(
        fx.input
            .run
            .context
            .flow
            .filing_receipts()
            .unwrap()
            .pending()
            .count(),
        1
    );
    assert_eq!(fx.input.run.context.budget.gauges().0, 1);
    fx.input.drain_trace();
    let complete = fx.complete(None);
    assert_eq!(complete.receipts().dispositions().count(), 1);
    assert_eq!(fx.input.records(Kind::StateStoreMeasurement).len(), 2);
    assert!(
        fx.input
            .records(Kind::StateFlow)
            .iter()
            .any(|record| record["payload"]["phase"] == "filed")
    );
}

#[test]
fn zero_text_projection_finishes_without_a_committed_source() {
    let mut fx = ReceiptFixture::new(100);
    fx.begin(event(&[]));
    let complete = fx.complete(None);
    assert_eq!(complete.receipts().sources().count(), 0);
    assert_eq!(complete.receipts().dispositions().count(), 0);
    assert!(fx.file_requests.is_empty());
    fx.input.no_request();
}

#[test]
fn cancellation_before_admission_returns_original_event_and_leaves_the_callers_wait_alone() {
    let mut fx = ReceiptFixture::new(100);
    let original = event(&["Read cancelled."]);
    let expected = format!("{original:?}");
    fx.begin(original);
    fx.input.run.context.cancel_filing();
    fx.input.run.context.cancel_filing();
    let cancellation = fx.input.run.context.take_cancelled_filing().unwrap();
    assert_eq!(format!("{:?}", cancellation.event.unwrap()), expected);
    assert!(cancellation.sequence.is_none());
    assert!(cancellation.receipts.is_none());
    assert!(fx.input.records(Kind::MessageAssistant).is_empty());
    assert_eq!(
        fx.input.run.context.deadlines.get(PumpDeadline::Decision),
        Some(fx.bound)
    );
}

#[test]
fn cancellation_of_a_pending_author_resumes_only_that_ticket_and_never_grants_receipts() {
    let mut fx = ReceiptFixture::new(100);
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
    fx.begin(event(&["Read cancelled after ticket."]));
    assert!(matches!(
        fx.advance(),
        Err(RunFailure::Wait {
            key: PumpDeadline::Label,
            ..
        })
    ));
    let ticket = fx.input.run.context.pending_author().unwrap().ticket;
    fx.input.run.context.cancel_filing();
    assert!(fx.input.run.context.take_cancelled_filing().is_none());
    assert_eq!(
        fx.input.run.context.pending_author().unwrap().ticket,
        ticket
    );
    fx.input.run.context.complete_wait(PumpDeadline::Label);
    fx.input.run.context.complete_wait(PumpDeadline::Decision);
    fx.input.drain_trace();
    assert_eq!(fx.advance().unwrap(), FilingProgress::Cancelled);
    let cancelled = fx.input.run.context.take_cancelled_filing().unwrap();
    assert!(cancelled.sequence.is_some());
    assert!(cancelled.event.is_none());
    assert!(
        fx.input.run.context.flow.ledger().facts().next().is_some(),
        "cancellation does not erase the filing"
    );
    assert_eq!(fx.input.records(Kind::MessageAssistant).len(), 1);
    assert!(fx.input.run.context.failure().is_none());
}

#[test]
fn missing_or_replaced_enclosing_wait_cannot_license_pending_receipts() {
    for replace in [false, true] {
        let mut fx = ReceiptFixture::new(100);
        fx.begin(event(&["Read pending."]));
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
        // Direct corruption is separate from the public retirement refusal.
        fx.input
            .run
            .context
            .deadlines
            .disarm(PumpDeadline::Decision);
        if replace {
            fx.input
                .run
                .context
                .arm(
                    PumpDeadline::Decision,
                    AbsoluteBound::after(104, 2_000_000).unwrap(),
                    &mut fx.input.run.clock,
                )
                .unwrap();
        }
        assert!(matches!(
            fx.advance(),
            Err(RunFailure::Flow {
                reason: FlowHaltReason::ProtocolFailed,
                ..
            })
        ));
        assert!(fx.input.run.context.waiting_content.is_some());
        assert!(fx.input.records(Kind::MessageAssistant).is_empty());
    }
}

#[test]
fn timely_complete_receipts_survive_retired_enclosing_wait_and_delayed_take() {
    let mut fx = ReceiptFixture::new(100);
    let author_time = fx.input.run.clock.now().unwrap();
    fx.begin(event(&["Read complete."]));
    for _ in 0..128 {
        if fx.step(None) == FilingProgress::Complete {
            break;
        }
    }
    let recorded = fx.input.run.context.flow.filing_recorded_ns().unwrap();
    assert!(
        recorded > author_time,
        "completion follows the member receipt"
    );
    assert_eq!(recorded, fx.input.run.clock.now().unwrap());
    assert!(recorded < fx.bound.deadline_ns());
    fx.input.run.context.complete_wait(PumpDeadline::Decision);
    fx.input
        .run
        .clock
        .advance_to(fx.bound.deadline_ns() + 1)
        .unwrap();
    let complete = fx
        .input
        .run
        .context
        .take_filing(&mut fx.input.run.clock)
        .unwrap()
        .unwrap();
    assert_eq!(complete.recorded_ns(), recorded);
    assert_eq!(complete.receipts().dispositions().count(), 1);
}

#[test]
fn a_withheld_member_receipt_expires_the_original_barrier_and_retains_exact_owed_source() {
    let mut fx = ReceiptFixture::new(100);
    fx.begin(event(&["Read unanswered."]));
    for _ in 0..128 {
        fx.advance().unwrap();
        fx.classifier_reply(None);
        fx.member(false);
        if !fx.file_requests.is_empty() {
            break;
        }
    }
    assert_eq!(fx.file_requests.len(), 1);
    let request = &fx.file_requests[0];
    let s::StateFrame::File { filing, .. } = request else {
        panic!("statement request");
    };
    let source = filing.block_id.clone();
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
    let owed = fx.input.run.context.flow.ledger().facts().next().unwrap();
    assert_eq!(owed.source, &source);
    assert_eq!(owed.state, LedgerState::AwaitingReceipt);
    assert!(
        fx.input
            .run
            .context
            .flow
            .filing_receipts()
            .unwrap()
            .pending()
            .any(|s| s == &source)
    );
    assert!(
        fx.input
            .run
            .context
            .take_filing(&mut fx.input.run.clock)
            .is_err()
    );
    let mut bytes = [0; 16];
    assert!(
        matches!(fx.input.run.state.read(&mut bytes), Err(error) if error.kind() == std::io::ErrorKind::WouldBlock)
    );
}

fn complete_but_owned(fx: &mut ReceiptFixture) {
    fx.begin(event(&["Read ownership."]));
    for _ in 0..128 {
        if fx.step(None) == FilingProgress::Complete {
            return;
        }
    }
    panic!("fixture did not finish its retained consumer");
}

#[test]
fn an_untaken_consumer_excludes_input_capture_templates_and_another_author() {
    // Each competing entry uses a fresh otherwise-ready context so a refusal
    // cannot be attributed to a preceding test's failure latch or held source.
    let mut input = ReceiptFixture::new(100);
    complete_but_owned(&mut input);
    let frame = weaver_types::TurnFrame::carry_from(br#"{"text":"Read next."}"#, 42);
    let refused = input
        .input
        .run
        .context
        .begin_input(frame.clone(), &mut input.input.run.clock)
        .unwrap_err();
    assert_eq!(refused.frame, frame);
    assert!(refused.failure.is_none());
    assert!(
        input
            .input
            .run
            .context
            .flow
            .ledger()
            .facts()
            .next()
            .is_none()
    );

    let mut capture = ReceiptFixture::new(100);
    complete_but_owned(&mut capture);
    let began = capture.input.run.clock.now().unwrap();
    let result = capture.input.run.context.prepare_member_capture(
        1.into(),
        s::SaveWindow {
            began_ns: began.into(),
            deadline_ns: (began + 1_000_000).into(),
        },
        &mut capture.input.run.clock,
    );
    assert!(result.is_err());
    assert!(capture.input.run.context.capture_snapshot().is_none());
    assert!(
        capture
            .input
            .run
            .context
            .flow
            .filing_receipts()
            .unwrap()
            .is_complete()
    );

    let mut templates = ReceiptFixture::new(100);
    complete_but_owned(&mut templates);
    templates
        .input
        .run
        .context
        .arm(
            PumpDeadline::Enter,
            AbsoluteBound::after(104, 2_000_000).unwrap(),
            &mut templates.input.run.clock,
        )
        .unwrap();
    let inventory = crate::flow::templates::CompiledInventory::new(&[]).unwrap();
    let error = templates
        .input
        .run
        .context
        .begin_template_checks(inventory, &mut templates.input.run.clock)
        .unwrap_err();
    assert_eq!(error.refusal, TemplateBeginRefusal::Busy);
    assert!(templates.input.run.context.template_transcript().is_none());

    let mut other = ReceiptFixture::new(100);
    complete_but_owned(&mut other);
    let next = event(&["Read forbidden second author."]);
    let expected = format!("{next:?}");
    assert!(other.input.run.author(next).is_err());
    assert_eq!(
        format!("{:?}", other.input.run.context.refused_author().unwrap()),
        expected
    );
    assert_eq!(other.input.records(Kind::MessageAssistant).len(), 1);
    assert!(other.input.run.context.filing.is_some());
}

#[test]
fn generic_resume_cannot_take_the_receipt_owners_pending_author() {
    let mut fx = ReceiptFixture::new(100);
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
    fx.begin(event(&["Read exact original."]));
    assert!(matches!(
        fx.advance(),
        Err(RunFailure::Wait {
            key: PumpDeadline::Label,
            ..
        })
    ));
    let ticket = fx.input.run.context.pending_author().unwrap().ticket;
    let original = format!("{:?}", fx.input.run.context.pending_author().unwrap().event);
    fx.input.run.context.complete_wait(PumpDeadline::Label);
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
    assert_eq!(
        format!("{:?}", fx.input.run.context.pending_author().unwrap().event),
        original
    );
    assert!(fx.input.records(Kind::MessageAssistant).is_empty());
}
