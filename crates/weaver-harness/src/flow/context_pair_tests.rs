//! Never-dispatched pair accounting through the real recorder and Unix sockets.
//! The checker and member replies are scripted mechanics, not model/store qualification.

use super::output_tests::{OutputFixture, original, text};
use super::*;
use crate::flow::coordinator::{
    OriginalRequests, RequestScheme, TransportSource, tests::operation,
};
use crate::flow::ledger::LedgerFile;
use crate::flow::pump::LifecyclePorts;
use sha2::{Digest as _, Sha256};
use std::io::{Read, Write};
use weaver_traits::{ContentBlock, ToolCall};
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

fn call(arguments: &str) -> ContentBlock {
    ContentBlock::ToolCall(ToolCall {
        name: "sh".into(),
        arguments: arguments.into(),
    })
}

fn scheme() -> (String, s::SchemeDeclaration, RequestScheme) {
    let seed: s::SeedRequest = s::decode_strict(include_str!("fixtures/seed.json")).unwrap();
    let mut artifact: serde_json::Value = serde_json::from_str(&seed.ste100_scheme).unwrap();
    let mut recall = artifact["verbs"][0].clone();
    recall["key"] = serde_json::json!("recall");
    recall["forms"] = serde_json::json!(["Recall"]);
    artifact["verbs"].as_array_mut().unwrap().push(recall);
    artifact["recall_keys"] = serde_json::json!(["recall"]);
    let text = serde_json::to_string(&artifact).unwrap();
    let digest = s::Digest::new(format!("{:x}", Sha256::digest(text.as_bytes()))).unwrap();
    let declaration = s::SchemeDeclaration {
        scheme_id: format!("ste100:{digest}"),
        artifact_sha256: digest,
        verbs: vec!["read".into(), "recall".into()],
    };
    let parsed = RequestScheme::from_exact_scheme(&text, &declaration).unwrap();
    (text, declaration, parsed)
}

fn checker_reply(request: &s::Ste100Request) -> s::Ste100Reply {
    let result = if request.text == "bad" {
        s::Ste100Result::NotSte100 {
            code: s::Ste100DefectCode::NonSte100,
            detail: "scripted defect".into(),
            spans: vec![],
        }
    } else {
        let mut offset = 0;
        let bindings = request
            .text
            .split_inclusive('.')
            .map(|raw| {
                let sentence = raw.trim_start();
                let start = offset + raw.len() - sentence.len();
                offset += raw.len();
                assert!(sentence.ends_with('.'));
                let word = sentence.find(' ').unwrap();
                let verb = sentence[..word].to_lowercase();
                assert!(matches!(verb.as_str(), "read" | "recall"));
                s::Binding {
                    statement_span: s::Span {
                        start: (start as u64).into(),
                        end: (offset as u64).into(),
                    },
                    verb,
                    verb_span: s::Span {
                        start: (start as u64).into(),
                        end: ((start + word) as u64).into(),
                    },
                    condition: None,
                    condition_span: None,
                    nouns: vec![s::NounBinding {
                        value: sentence[word + 1..sentence.len() - 1].into(),
                        span: s::Span {
                            start: ((start + word + 1) as u64).into(),
                            end: ((offset - 1) as u64).into(),
                        },
                    }],
                }
            })
            .collect();
        s::Ste100Result::Accepted { bindings }
    };
    s::Ste100Reply {
        request_id: request.request_id,
        target: request.target.clone(),
        scheme_id: request.scheme_id.clone(),
        result,
    }
}

#[derive(Clone, Copy)]
enum Hold {
    None,
    Statement,
    Opening,
    Terminal,
}

struct PairFixture {
    output: OutputFixture,
    scheme: RequestScheme,
    bytes: Vec<u8>,
    requests: Vec<s::StateFrame>,
    answered: Vec<bool>,
    operation: u64,
}

impl PairFixture {
    fn new() -> Self {
        let (text, declaration, scheme) = scheme();
        Self {
            output: OutputFixture::with_scheme(text, declaration),
            scheme,
            bytes: vec![],
            requests: vec![],
            answered: vec![],
            operation: 2,
        }
    }

    fn checker(&mut self) {
        if let Some(request) = self.output.receive_check() {
            self.output.input.reply(&checker_reply(&request));
        }
    }

    fn member(&mut self, hold: Hold) {
        self.output.transport(TransportSource::State);
        let mut bytes = [0; 65536];
        loop {
            match self.output.input.run.state.read(&mut bytes) {
                Ok(0) => panic!("member socket closed"),
                Ok(n) => self.bytes.extend_from_slice(&bytes[..n]),
                Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => break,
                Err(error) => panic!("member fixture read: {error}"),
            }
        }
        while let Some(end) = self.bytes.iter().position(|byte| *byte == b'\n') {
            let line: Vec<_> = self.bytes.drain(..=end).collect();
            let request = s::decode_state_line(&line, s::FrameDirection::Request).unwrap();
            assert!(matches!(
                request,
                s::StateFrame::File { .. } | s::StateFrame::FilePair { .. }
            ));
            self.requests.push(request);
            self.answered.push(false);
        }
        for index in 0..self.requests.len() {
            if self.answered[index] {
                continue;
            }
            let held = match (&self.requests[index], hold) {
                (s::StateFrame::File { .. }, Hold::Statement) => true,
                (s::StateFrame::FilePair { filing, .. }, Hold::Opening) => {
                    matches!(filing.pair, s::PairRecord::Started(_))
                }
                (s::StateFrame::FilePair { filing, .. }, Hold::Terminal) => {
                    matches!(filing.pair, s::PairRecord::Ended(_))
                }
                _ => false,
            };
            if !held {
                self.answer(index);
            }
        }
        self.output.transport(TransportSource::State);
    }

    fn answer(&mut self, index: usize) {
        assert!(!self.answered[index]);
        let request = &self.requests[index];
        let block_id = match request {
            s::StateFrame::File { filing, .. } => filing.block_id.clone(),
            s::StateFrame::FilePair { filing, .. } => filing.block_id.clone(),
            _ => unreachable!(),
        };
        let began = self.output.input.run.clock.now().unwrap() + 1;
        let response = s::StateFrame::Filed {
            common: request.common().clone(),
            block_id,
            observation: operation(request, self.operation, began),
        };
        self.operation += 1;
        self.output.input.run.clock.advance_to(began + 1).unwrap();
        self.output
            .input
            .run
            .state
            .write_all(&s::encode_state_line(&response, s::FrameDirection::Response).unwrap())
            .unwrap();
        self.answered[index] = true;
    }

    fn generation(&mut self, content: Vec<ContentBlock>) -> OriginalRequests {
        self.output.begin(original(content));
        for _ in 0..256 {
            let progress = self.output.advance().unwrap();
            self.checker();
            self.member(Hold::Statement);
            if progress == OutputProgress::Complete {
                let checked = self
                    .output
                    .input
                    .run
                    .context
                    .take_output(&mut self.output.input.run.clock)
                    .unwrap()
                    .unwrap();
                return OriginalRequests::from_checked(checked, &self.scheme).unwrap();
            }
        }
        panic!("generation did not finish its real recorded checks");
    }

    fn begin(&mut self, original: OriginalRequests, cause: s::NonDispatchCause) {
        self.output
            .input
            .run
            .context
            .begin_not_dispatched(original, cause, &mut self.output.input.run.clock)
            .unwrap();
    }

    fn advance(&mut self) -> Result<PairProgress, RunFailure> {
        self.output
            .input
            .run
            .context
            .advance_pairs(&mut ports(&mut self.output.input.run.clock))
    }

    fn step(&mut self, hold: Hold) -> PairProgress {
        let progress = self.advance().unwrap();
        self.checker();
        self.member(hold);
        progress
    }

    fn complete(&mut self) {
        for _ in 0..512 {
            if self.step(Hold::None) == PairProgress::Complete {
                return;
            }
        }
        panic!("pair accounting did not settle");
    }

    fn until_held(&mut self, hold: Hold) {
        for _ in 0..256 {
            self.step(hold);
            if self
                .requests
                .iter()
                .zip(&self.answered)
                .any(|(request, answered)| {
                    !*answered
                        && match (request, hold) {
                            (s::StateFrame::File { .. }, Hold::Statement) => true,
                            (s::StateFrame::FilePair { filing, .. }, Hold::Opening) => {
                                matches!(filing.pair, s::PairRecord::Started(_))
                            }
                            (s::StateFrame::FilePair { filing, .. }, Hold::Terminal) => {
                                matches!(filing.pair, s::PairRecord::Ended(_))
                            }
                            _ => false,
                        }
                })
            {
                return;
            }
        }
        panic!("requested member receipt did not reach its withheld boundary");
    }
}

#[test]
fn every_never_dispatched_cause_records_and_commits_each_accepted_external_pair() {
    for cause in [
        s::NonDispatchCause::PassPurpose,
        s::NonDispatchCause::RepairExhausted,
        s::NonDispatchCause::PlanBreak,
        s::NonDispatchCause::RoundLimit,
        s::NonDispatchCause::Stop,
        s::NonDispatchCause::Unload,
        s::NonDispatchCause::NoGate,
    ] {
        let mut fx = PairFixture::new();
        let original = fx.generation(vec![
            text("Read first."),
            call("same opaque command"),
            text("Read second."),
            call("same opaque command"),
            text("bad"),
            call("unresolved command"),
        ]);
        let calls: Vec<_> = original
            .requests()
            .iter()
            .map(|request| request.opening().call_id.clone())
            .collect();
        assert_eq!(calls.len(), 2);
        assert_eq!(original.unresolved().len(), 1);
        fx.begin(original, cause);
        fx.complete();
        for (ordinal, call) in calls.iter().enumerate() {
            let completed = fx
                .output
                .input
                .run
                .context
                .completed_pair(call, &mut fx.output.input.run.clock)
                .unwrap()
                .unwrap();
            assert_eq!(completed.started().call_id.call.0, ordinal as u64);
            assert_eq!(
                completed.ended().reason,
                Some(s::PairFailure::NotDispatched { cause })
            );
            assert!(completed.ended().execution.is_none());
            assert!(completed.ended().outcome.is_none());
            assert!(completed.ended().loaded_from.is_none());
            assert_eq!(completed.receipt().block_id(), completed.terminal_source());
            assert_eq!(
                completed.receipt().request_id(),
                completed.terminal().common().request_id
            );
            assert!(
                matches!(completed.opening().file(), LedgerFile::Pair(file) if matches!(&file.pair, s::PairRecord::Started(start) if &start.call_id == call))
            );
            assert!(
                matches!(completed.terminal().file(), LedgerFile::Pair(file) if matches!(&file.pair, s::PairRecord::Ended(end) if &end.call_id == call))
            );
            assert_eq!(
                completed.statement().source(),
                &completed.started().statement.block_id
            );
        }
        assert_eq!(
            fx.requests
                .iter()
                .filter(|r| matches!(r, s::StateFrame::File { .. }))
                .count(),
            2
        );
        assert_eq!(
            fx.requests
                .iter()
                .filter(|r| matches!(r, s::StateFrame::FilePair { .. }))
                .count(),
            4
        );
        assert_eq!(
            fx.output.requests.len(),
            3,
            "pair metadata is never classified"
        );
        assert_eq!(fx.output.input.records(Kind::ToolCallStarted).len(), 2);
        assert_eq!(fx.output.input.records(Kind::ToolCallCompleted).len(), 2);
        let returned = fx
            .output
            .input
            .run
            .context
            .take_pair_accounting(&mut fx.output.input.run.clock)
            .unwrap()
            .unwrap();
        assert_eq!(returned.unresolved().len(), 1);
        assert!(fx.output.input.run.context.outstanding_pairs().is_empty());
        assert!(fx.output.input.run.context.failure().is_none());
    }
}

#[test]
fn two_recalls_share_one_statement_receipt_without_duplicate_classification_or_filing() {
    let mut fx = PairFixture::new();
    let original = fx.generation(vec![text("Recall first. Recall second.")]);
    let calls: Vec<_> = original
        .requests()
        .iter()
        .map(|request| request.opening().call_id.clone())
        .collect();
    assert_eq!(calls.len(), 2);
    fx.begin(original, s::NonDispatchCause::PassPurpose);
    fx.complete();
    let first = fx
        .output
        .input
        .run
        .context
        .completed_pair(&calls[0], &mut fx.output.input.run.clock)
        .unwrap()
        .unwrap();
    let source = first.statement().source().clone();
    let receipt = first.statement().common().clone();
    assert_eq!(first.started().statement.binding.0, 0);
    let second = fx
        .output
        .input
        .run
        .context
        .completed_pair(&calls[1], &mut fx.output.input.run.clock)
        .unwrap()
        .unwrap();
    assert_eq!(second.started().statement.binding.0, 1);
    assert_eq!(second.statement().source(), &source);
    assert_eq!(second.statement().common(), &receipt);
    assert_eq!(
        fx.requests
            .iter()
            .filter(|r| matches!(r, s::StateFrame::File { .. }))
            .count(),
        1
    );
    assert_eq!(
        fx.requests
            .iter()
            .filter(|r| matches!(r, s::StateFrame::FilePair { .. }))
            .count(),
        4
    );
    assert_eq!(fx.output.requests.len(), 1);
}

#[test]
fn each_missing_receipt_keeps_the_original_wait_and_never_grants_completion() {
    for hold in [Hold::Statement, Hold::Opening, Hold::Terminal] {
        let mut fx = PairFixture::new();
        let original = fx.generation(vec![text("Read held."), call("opaque")]);
        let call = original.requests()[0].opening().call_id.clone();
        fx.begin(original, s::NonDispatchCause::RoundLimit);
        assert!(
            fx.output
                .input
                .run
                .context
                .complete_wait(PumpDeadline::Decision)
                .is_none()
        );
        fx.until_held(hold);
        assert!(
            fx.output
                .input
                .run
                .context
                .completed_pair(&call, &mut fx.output.input.run.clock)
                .unwrap()
                .is_none()
        );
        assert!(
            fx.output
                .input
                .run
                .context
                .take_pair_accounting(&mut fx.output.input.run.clock)
                .unwrap()
                .is_none()
        );
        assert!(
            fx.output
                .input
                .run
                .context
                .complete_wait(PumpDeadline::Decision)
                .is_none()
        );
        assert_eq!(
            fx.output
                .input
                .run
                .context
                .deadlines
                .get(PumpDeadline::Decision),
            Some(fx.output.bound)
        );
        let unanswered = fx
            .requests
            .iter()
            .zip(&fx.answered)
            .filter(|(_, answered)| !**answered)
            .count();
        assert_eq!(unanswered, 1);
        fx.output
            .input
            .run
            .clock
            .advance_to(fx.output.bound.deadline_ns())
            .unwrap();
        assert!(matches!(
            fx.advance(),
            Err(RunFailure::Flow {
                reason: FlowHaltReason::DecisionBound,
                ..
            })
        ));
        assert!(
            fx.output
                .input
                .run
                .context
                .completed_pair(&call, &mut fx.output.input.run.clock)
                .is_err()
        );
        assert!(
            fx.output
                .input
                .run
                .context
                .take_pair_accounting(&mut fx.output.input.run.clock)
                .is_err()
        );
    }
}

#[test]
fn a_terminal_member_answer_is_not_a_completed_pair_until_its_recording_barriers_finish() {
    let mut fx = PairFixture::new();
    let original = fx.generation(vec![text("Read final."), call("opaque")]);
    let call = original.requests()[0].opening().call_id.clone();
    fx.begin(original, s::NonDispatchCause::NoGate);
    fx.until_held(Hold::Terminal);
    let index = fx.answered.iter().position(|answered| !answered).unwrap();
    fx.answer(index);
    fx.output.transport(TransportSource::State);
    assert!(
        fx.output
            .input
            .run
            .context
            .completed_pair(&call, &mut fx.output.input.run.clock)
            .unwrap()
            .is_none()
    );
    assert_eq!(fx.output.input.records(Kind::ToolCallCompleted).len(), 1);
    fx.output.input.fill_trace();
    assert_ne!(fx.advance().unwrap(), PairProgress::Complete);
    assert!(
        fx.output
            .input
            .run
            .context
            .completed_pair(&call, &mut fx.output.input.run.clock)
            .unwrap()
            .is_none()
    );
    fx.output.input.drain_trace();
    fx.complete();
    assert!(
        fx.output
            .input
            .run
            .context
            .completed_pair(&call, &mut fx.output.input.run.clock)
            .unwrap()
            .is_some()
    );
}

#[test]
fn stop_and_unload_update_unclaimed_endings_without_rewriting_a_claimed_terminal() {
    let mut fx = PairFixture::new();
    let original = fx.generation(vec![text("Read stop."), call("opaque")]);
    let call_id = original.requests()[0].opening().call_id.clone();
    fx.begin(original, s::NonDispatchCause::RoundLimit);
    fx.until_held(Hold::Opening);
    fx.output
        .input
        .run
        .context
        .bar_pending_pairs(s::NonDispatchCause::Stop, &mut fx.output.input.run.clock)
        .unwrap();
    fx.output
        .input
        .run
        .context
        .bar_pending_pairs(s::NonDispatchCause::Unload, &mut fx.output.input.run.clock)
        .unwrap();
    fx.output
        .input
        .run
        .context
        .bar_pending_pairs(s::NonDispatchCause::Stop, &mut fx.output.input.run.clock)
        .unwrap();
    fx.until_held(Hold::Terminal);
    fx.output
        .input
        .run
        .context
        .bar_pending_pairs(
            s::NonDispatchCause::RoundLimit,
            &mut fx.output.input.run.clock,
        )
        .unwrap();
    fx.complete();
    let completed = fx
        .output
        .input
        .run
        .context
        .completed_pair(&call_id, &mut fx.output.input.run.clock)
        .unwrap()
        .unwrap();
    assert_eq!(
        completed.ended().reason,
        Some(s::PairFailure::NotDispatched {
            cause: s::NonDispatchCause::Unload
        })
    );
    assert_eq!(fx.output.input.records(Kind::ToolCallCompleted).len(), 1);

    let mut claimed = PairFixture::new();
    let original = claimed.generation(vec![text("Read immutable."), call("opaque")]);
    let call_id = original.requests()[0].opening().call_id.clone();
    claimed.begin(original, s::NonDispatchCause::RoundLimit);
    claimed.until_held(Hold::Terminal);
    claimed
        .output
        .input
        .run
        .context
        .bar_pending_pairs(
            s::NonDispatchCause::Unload,
            &mut claimed.output.input.run.clock,
        )
        .unwrap();
    claimed.complete();
    let completed = claimed
        .output
        .input
        .run
        .context
        .completed_pair(&call_id, &mut claimed.output.input.run.clock)
        .unwrap()
        .unwrap();
    assert_eq!(
        completed.ended().reason,
        Some(s::PairFailure::NotDispatched {
            cause: s::NonDispatchCause::RoundLimit,
        })
    );
}

#[test]
fn foreign_receipt_custody_is_refused_without_consuming_the_original_generation() {
    let mut source = PairFixture::new();
    let mut foreign = PairFixture::new();
    let original = source.generation(vec![text("Read owner."), call("opaque")]);
    let refused = foreign
        .output
        .input
        .run
        .context
        .begin_not_dispatched(
            original,
            s::NonDispatchCause::Stop,
            &mut foreign.output.input.run.clock,
        )
        .unwrap_err();
    assert_eq!(refused.refusal, PairBeginRefusal::LostCustody);
    assert!(refused.failure.is_none());
    assert!(
        foreign
            .output
            .input
            .records(Kind::ToolCallStarted)
            .is_empty()
    );
    source.begin(refused.original, s::NonDispatchCause::Stop);
    source.complete();
    assert!(source.output.input.run.context.failure().is_none());
}

#[test]
fn ordinary_filing_and_input_apis_cannot_steal_the_pair_owned_filing() {
    let mut fx = PairFixture::new();
    let original = fx.generation(vec![text("Read owner."), call("opaque")]);
    let call = original.requests()[0].opening().call_id.clone();
    fx.begin(original, s::NonDispatchCause::PlanBreak);
    fx.until_held(Hold::Opening);
    assert!(matches!(
        fx.output
            .input
            .run
            .context
            .advance_filing(&mut ports(&mut fx.output.input.run.clock)),
        Err(RunFailure::PairOwned)
    ));
    assert!(matches!(
        fx.output
            .input
            .run
            .context
            .take_filing(&mut fx.output.input.run.clock),
        Err(RunFailure::PairOwned)
    ));
    fx.output.input.run.context.cancel_filing();
    assert!(
        fx.output
            .input
            .run
            .context
            .take_cancelled_filing()
            .is_none()
    );
    let frame = weaver_types::TurnFrame::carry_from(br#"{"text":"Read next."}"#, 42);
    let refused = fx
        .output
        .input
        .run
        .context
        .begin_input(frame.clone(), &mut fx.output.input.run.clock)
        .unwrap_err();
    assert_eq!(refused.frame, frame);
    assert!(refused.failure.is_none());
    fx.complete();
    assert!(
        fx.output
            .input
            .run
            .context
            .completed_pair(&call, &mut fx.output.input.run.clock)
            .unwrap()
            .is_some()
    );
    assert!(fx.output.input.run.context.failure().is_none());
}

#[test]
fn completed_accounting_can_be_taken_after_its_original_wait_is_retired() {
    let mut fx = PairFixture::new();
    let original = fx.generation(vec![text("Read complete."), call("opaque")]);
    let call = original.requests()[0].opening().call_id.clone();
    fx.begin(original, s::NonDispatchCause::PassPurpose);
    fx.complete();
    fx.output
        .input
        .run
        .context
        .complete_wait(PumpDeadline::Decision)
        .unwrap();
    fx.output
        .input
        .run
        .clock
        .advance_to(fx.output.bound.deadline_ns() + 1)
        .unwrap();
    let returned = fx
        .output
        .input
        .run
        .context
        .take_pair_accounting(&mut fx.output.input.run.clock)
        .unwrap()
        .unwrap();
    assert_eq!(returned.requests()[0].opening().call_id, call);
    assert!(
        fx.output
            .input
            .run
            .context
            .completed_pair(&call, &mut fx.output.input.run.clock)
            .unwrap()
            .is_some()
    );
    assert!(
        fx.output
            .input
            .run
            .context
            .take_pair_accounting(&mut fx.output.input.run.clock)
            .unwrap()
            .is_none()
    );
}

#[test]
fn a_pending_ordinary_measurement_before_the_first_opening_is_progress_not_failure() {
    let mut fx = PairFixture::new();
    let original = fx.generation(vec![text("Read background."), call("opaque")]);
    let call = original.requests()[0].opening().call_id.clone();
    fx.begin(original, s::NonDispatchCause::NoGate);
    // Complete the original statement's member exchange before the pair driver
    // starts. Its real observation still needs the recorder's next free slot.
    for _ in 0..64 {
        fx.output
            .input
            .run
            .context
            .reduce_flow(&mut fx.output.input.run.clock)
            .unwrap();
        fx.member(Hold::None);
        if fx.answered.iter().any(|answered| *answered) {
            break;
        }
    }
    assert!(fx.answered.iter().any(|answered| *answered));
    fx.output.input.fill_trace();
    for _ in 0..64 {
        fx.output
            .input
            .run
            .context
            .reduce_flow(&mut fx.output.input.run.clock)
            .unwrap();
        if fx.output.input.run.context.flow.has_pending_record() {
            break;
        }
    }
    assert!(fx.output.input.run.context.flow.has_pending_record());
    assert_ne!(fx.advance().unwrap(), PairProgress::Complete);
    assert!(fx.output.input.run.context.failure().is_none());
    assert!(fx.output.input.records(Kind::ToolCallStarted).is_empty());
    fx.output.input.drain_trace();
    fx.complete();
    assert!(
        fx.output
            .input
            .run
            .context
            .completed_pair(&call, &mut fx.output.input.run.clock)
            .unwrap()
            .is_some()
    );
}

fn pair_halt_facts(fx: &PairFixture) -> super::super::halt_snapshot::ExternalFacts {
    super::super::halt_snapshot::ExternalFacts {
        outstanding_pairs: fx.output.input.run.context.outstanding_pairs(),
        capture: None,
        wait: fx.output.input.run.context.halt_wait().cloned(),
        input_id: None,
    }
}

#[test]
fn halt_snapshot_refuses_omitted_recorded_opening_and_accepts_its_exact_obligations() {
    let mut fx = PairFixture::new();
    let original = fx.generation(vec![text("Read retained."), call("opaque")]);
    let call_id = original.requests()[0].opening().call_id.clone();
    fx.begin(original, s::NonDispatchCause::Stop);
    fx.until_held(Hold::Opening);
    let outstanding = fx.output.input.run.context.outstanding_pairs();
    assert_eq!(outstanding.len(), 1);
    assert_eq!(outstanding[0].call_id, call_id);
    assert!(!outstanding[0].opening_filed);
    assert!(!outstanding[0].terminal_filed);
    assert!(outstanding[0].terminal_source.is_none());
    fx.output
        .input
        .run
        .clock
        .advance_to(fx.output.bound.deadline_ns())
        .unwrap();
    assert!(matches!(
        fx.advance(),
        Err(RunFailure::Flow {
            reason: FlowHaltReason::DecisionBound,
            ..
        })
    ));
    let mut omitted = pair_halt_facts(&fx);
    omitted.outstanding_pairs.clear();
    let HaltAccountFailure::Snapshot(rejected) = fx
        .output
        .input
        .run
        .context
        .freeze_halt_account(omitted)
        .unwrap_err()
    else {
        panic!("omitted pair must fail the retained-owner check");
    };
    assert!(rejected.external.outstanding_pairs.is_empty());
    assert!(
        fx.output
            .input
            .run
            .context
            .halt()
            .unwrap()
            .account()
            .is_none()
    );
    let actual = pair_halt_facts(&fx);
    assert_eq!(actual.outstanding_pairs, outstanding);
    fx.output
        .input
        .run
        .context
        .freeze_halt_account(actual)
        .unwrap();
    assert_eq!(
        fx.output
            .input
            .run
            .context
            .halt()
            .unwrap()
            .account()
            .unwrap()
            .outstanding_pairs,
        outstanding,
    );
    assert!(
        fx.output
            .input
            .run
            .context
            .completed_pair(&call_id, &mut fx.output.input.run.clock,)
            .is_err()
    );
}

#[test]
fn actual_pair_filing_refusal_latches_failure_without_terminal_or_completion() {
    let mut fx = PairFixture::new();
    let original = fx.generation(vec![text("Read refused."), call("opaque")]);
    let call_id = original.requests()[0].opening().call_id.clone();
    fx.begin(original, s::NonDispatchCause::NoGate);
    fx.until_held(Hold::Opening);
    let index = fx.answered.iter().position(|answered| !answered).unwrap();
    let request = &fx.requests[index];
    let s::StateFrame::FilePair { filing, .. } = request else {
        unreachable!()
    };
    let opening_source = filing.block_id.clone();
    let began = fx.output.input.run.clock.now().unwrap() + 1;
    let refused = s::StateFrame::Refused {
        common: request.common().clone(),
        code: s::StateRefusalCode::WriteFailed,
        detail: "scripted pair transaction refusal".into(),
        block_id: Some(opening_source.clone()),
        observation: operation(request, fx.operation, began),
    };
    fx.output.input.run.clock.advance_to(began + 1).unwrap();
    fx.output
        .input
        .run
        .state
        .write_all(&s::encode_state_line(&refused, s::FrameDirection::Response).unwrap())
        .unwrap();
    fx.answered[index] = true;
    fx.output.transport(TransportSource::State);
    // The refusal is a real answer whose measurement must be recorded before
    // it becomes a component failure. It never becomes a filing receipt.
    let failure = (0..128)
        .find_map(|_| fx.advance().err())
        .expect("a refused pair filing must halt");
    assert!(matches!(
        failure,
        RunFailure::Flow {
            reason: FlowHaltReason::FilingRefused,
            ..
        }
    ));
    assert!(
        fx.output
            .input
            .run
            .context
            .completed_pair(&call_id, &mut fx.output.input.run.clock,)
            .is_err()
    );
    assert!(
        fx.output
            .input
            .run
            .context
            .take_pair_accounting(&mut fx.output.input.run.clock,)
            .is_err()
    );
    assert!(fx.output.input.records(Kind::ToolCallCompleted).is_empty());
    let outstanding = fx.output.input.run.context.outstanding_pairs();
    assert_eq!(outstanding.len(), 1);
    assert_eq!(outstanding[0].opening_source, opening_source);
    assert!(!outstanding[0].opening_filed);
    assert!(outstanding[0].terminal_source.is_none());
    let actual = pair_halt_facts(&fx);
    fx.output
        .input
        .run
        .context
        .freeze_halt_account(actual)
        .unwrap();
    assert_eq!(
        fx.output
            .input
            .run
            .context
            .halt()
            .unwrap()
            .account()
            .unwrap()
            .outstanding_pairs,
        outstanding,
    );
}

#[test]
fn halt_preserves_recorded_terminal_receipt_without_issuing_a_completed_pair() {
    let mut fx = PairFixture::new();
    let original = fx.generation(vec![text("Read committed."), call("opaque")]);
    let call_id = original.requests()[0].opening().call_id.clone();
    fx.begin(original, s::NonDispatchCause::Stop);
    fx.until_held(Hold::Terminal);
    let index = fx.answered.iter().position(|answered| !answered).unwrap();
    let s::StateFrame::FilePair { filing, .. } = &fx.requests[index] else {
        unreachable!()
    };
    let terminal_source = filing.block_id.clone();
    fx.answer(index);
    fx.output.transport(TransportSource::State);
    // Reduce the ordinary filing path, but do not let the pair driver consume
    // its receipt. Failure can occur between these two owners' advances.
    for _ in 0..128 {
        fx.output
            .input
            .run
            .context
            .reduce_flow(&mut fx.output.input.run.clock)
            .unwrap();
        if fx
            .output
            .input
            .run
            .context
            .flow
            .filing_receipts()
            .is_some_and(|set| set.has_recorded_commit(&terminal_source))
        {
            break;
        }
    }
    assert!(
        fx.output
            .input
            .run
            .context
            .flow
            .filing_receipts()
            .is_some_and(|set| set.has_recorded_commit(&terminal_source))
    );
    assert!(
        fx.output
            .input
            .run
            .context
            .completed_pair(&call_id, &mut fx.output.input.run.clock,)
            .unwrap()
            .is_none()
    );
    fx.output
        .input
        .run
        .clock
        .advance_to(fx.output.bound.deadline_ns())
        .unwrap();
    assert!(matches!(
        fx.advance(),
        Err(RunFailure::Flow {
            reason: FlowHaltReason::DecisionBound,
            ..
        })
    ));
    let outstanding = fx.output.input.run.context.outstanding_pairs();
    assert_eq!(outstanding.len(), 1);
    assert!(outstanding[0].opening_filed);
    assert!(outstanding[0].terminal_filed);
    assert_eq!(
        outstanding[0].terminal_source.as_ref(),
        Some(&terminal_source)
    );
    let actual = pair_halt_facts(&fx);
    fx.output
        .input
        .run
        .context
        .freeze_halt_account(actual)
        .unwrap();
    assert_eq!(
        fx.output
            .input
            .run
            .context
            .halt()
            .unwrap()
            .account()
            .unwrap()
            .outstanding_pairs,
        outstanding,
    );
    assert!(
        fx.output
            .input
            .run
            .context
            .completed_pair(&call_id, &mut fx.output.input.run.clock,)
            .is_err()
    );
}

fn repaired_context_account(fx: &mut PairFixture) -> super::repair::RepairAccount {
    let original = fx.generation(vec![text("bad"), call("held original bytes"), text("bad")]);
    let source = original.unresolved()[0].rejected_source().clone();
    settle_pair_fixture_flow(fx);
    // These legal trace markers leave room for the private repair-owner fixture's
    // requested/instruction proofs. This test exercises the pair adapter and its
    // real recorder/socket custody, not the repair driver's instruction authoring.
    for _ in 0..2 {
        fx.output
            .input
            .run
            .author(AuthoredEvent {
                kind: Kind::Refusal,
                subsystem: weaver_trace::Subsystem::Harness,
                turn: Some(weaver_trace::TurnRef("output-turn".into())),
                source: crate::flow::author::AuthoredSource::Runtime,
                causal_parent: Some(Sequence(source.sequence.0)),
                payload: Some(Payload::Refusal(
                    serde_json::value::to_raw_value(
                        &serde_json::json!({"fixture":"private repair owner boundary"}),
                    )
                    .unwrap(),
                )),
            })
            .unwrap();
    }
    let correction = fx.generation(vec![text("Read corrected.")]);
    super::repair::account_fixture(original, vec![(source, correction)])
}

fn settle_pair_fixture_flow(fx: &mut PairFixture) {
    for _ in 0..256 {
        fx.output
            .input
            .run
            .context
            .reduce_flow(&mut fx.output.input.run.clock)
            .unwrap();
        fx.checker();
        fx.member(Hold::None);
        let context = &fx.output.input.run.context;
        if context.flow.ledger().facts().next().is_none()
            && !context.flow.has_pending_record()
            && context.flow.pending_check().is_none()
            && context.barrier.pending().is_none()
            && context.barrier.pending_author().is_none()
            && context.waiting_content.is_none()
        {
            return;
        }
    }
    panic!("pair fixture's ordinary flow did not settle");
}

#[test]
fn repaired_pair_uses_its_correction_lease_and_only_returns_to_the_repair_owner() {
    let mut fx = PairFixture::new();
    let account = repaired_context_account(&mut fx);
    let started = account.requests()[0].clone();
    assert_ne!(
        started.call_id.sequence,
        started.statement.block_id.sequence
    );
    fx.output
        .input
        .run
        .context
        .begin_repair_not_dispatched(
            account,
            s::NonDispatchCause::RepairExhausted,
            &mut fx.output.input.run.clock,
        )
        .unwrap();
    assert!(matches!(
        fx.output
            .input
            .run
            .context
            .take_pair_accounting(&mut fx.output.input.run.clock,),
        Err(RunFailure::PairOwned)
    ));
    fx.until_held(Hold::Statement);
    assert!(
        fx.output
            .input
            .run
            .context
            .take_repair_accounting(&mut fx.output.input.run.clock,)
            .unwrap()
            .is_none()
    );
    fx.complete();
    let account = fx
        .output
        .input
        .run
        .context
        .take_repair_accounting(&mut fx.output.input.run.clock)
        .unwrap()
        .unwrap();
    assert_eq!(account.requests(), std::slice::from_ref(&started));
    assert_eq!(account.generations().count(), 2);
    let completed = fx
        .output
        .input
        .run
        .context
        .completed_pair(&started.call_id, &mut fx.output.input.run.clock)
        .unwrap()
        .unwrap();
    assert_eq!(completed.started(), &started);
    assert_eq!(completed.statement().source(), &started.statement.block_id);
    assert_eq!(
        completed.ended().reason,
        Some(s::PairFailure::NotDispatched {
            cause: s::NonDispatchCause::RepairExhausted,
        })
    );
    assert_eq!(fx.output.input.records(Kind::ToolCallStarted).len(), 1);
    assert_eq!(fx.output.input.records(Kind::ToolCallCompleted).len(), 1);
    assert_eq!(fx.output.input.records(Kind::MessageAssistant).len(), 2);
}

#[test]
fn foreign_repaired_account_refuses_equal_wire_ids_without_consuming_custody() {
    let mut fx = PairFixture::new();
    let mut foreign = PairFixture::new();
    let local_account = repaired_context_account(&mut fx);
    let foreign_account = repaired_context_account(&mut foreign);
    assert_eq!(local_account.requests(), foreign_account.requests());
    let expected = foreign_account.requests()[0].clone();
    let error = fx
        .output
        .input
        .run
        .context
        .begin_repair_not_dispatched(
            foreign_account,
            s::NonDispatchCause::Stop,
            &mut fx.output.input.run.clock,
        )
        .unwrap_err();
    assert_eq!(error.refusal, PairBeginRefusal::LostCustody);
    assert!(error.failure.is_none());
    assert_eq!(error.account.requests(), &[expected]);
    assert_eq!(error.account.generations().count(), 2);
    assert!(fx.output.input.run.context.failure().is_none());
    fx.output
        .input
        .run
        .context
        .begin_repair_not_dispatched(
            local_account,
            s::NonDispatchCause::Stop,
            &mut fx.output.input.run.clock,
        )
        .unwrap();
    fx.complete();
}

#[test]
fn repair_take_cannot_consume_an_ordinary_pair_batch() {
    let mut fx = PairFixture::new();
    let original = fx.generation(vec![text("Read ordinary."), call("ordinary")]);
    let started = original.requests()[0].opening().clone();
    fx.begin(original, s::NonDispatchCause::PassPurpose);
    assert!(matches!(
        fx.output
            .input
            .run
            .context
            .take_repair_accounting(&mut fx.output.input.run.clock,),
        Err(RunFailure::PairOwned)
    ));
    fx.complete();
    let original = fx
        .output
        .input
        .run
        .context
        .take_pair_accounting(&mut fx.output.input.run.clock)
        .unwrap()
        .unwrap();
    assert_eq!(original.requests()[0].opening(), &started);
}

#[test]
fn repaired_receipt_waits_halt_at_the_original_bound_without_reauthoring() {
    for hold in [Hold::Statement, Hold::Opening, Hold::Terminal] {
        let mut fx = PairFixture::new();
        let account = repaired_context_account(&mut fx);
        let call = account.requests()[0].call_id.clone();
        fx.output
            .input
            .run
            .context
            .begin_repair_not_dispatched(
                account,
                s::NonDispatchCause::RepairExhausted,
                &mut fx.output.input.run.clock,
            )
            .unwrap();
        fx.until_held(hold);
        assert!(
            fx.output
                .input
                .run
                .context
                .complete_wait(PumpDeadline::Decision)
                .is_none()
        );
        assert!(
            fx.output
                .input
                .run
                .context
                .take_repair_accounting(&mut fx.output.input.run.clock,)
                .unwrap()
                .is_none()
        );
        let opened = fx.output.input.records(Kind::ToolCallStarted).len();
        let ended = fx.output.input.records(Kind::ToolCallCompleted).len();
        fx.output
            .input
            .run
            .clock
            .advance_to(fx.output.bound.deadline_ns())
            .unwrap();
        assert!(matches!(
            fx.advance(),
            Err(RunFailure::Flow {
                reason: FlowHaltReason::DecisionBound,
                ..
            })
        ));
        assert!(
            fx.output
                .input
                .run
                .context
                .take_repair_accounting(&mut fx.output.input.run.clock,)
                .is_err()
        );
        assert!(
            fx.output
                .input
                .run
                .context
                .completed_pair(&call, &mut fx.output.input.run.clock,)
                .is_err()
        );
        assert!(fx.advance().is_err());
        assert_eq!(fx.output.input.records(Kind::ToolCallStarted).len(), opened);
        assert_eq!(
            fx.output.input.records(Kind::ToolCallCompleted).len(),
            ended
        );
    }
}

#[test]
fn generic_cancellation_cannot_drop_or_duplicate_a_repaired_pair_filing() {
    let mut fx = PairFixture::new();
    let account = repaired_context_account(&mut fx);
    let call = account.requests()[0].call_id.clone();
    fx.output
        .input
        .run
        .context
        .begin_repair_not_dispatched(
            account,
            s::NonDispatchCause::RepairExhausted,
            &mut fx.output.input.run.clock,
        )
        .unwrap();
    fx.until_held(Hold::Opening);
    fx.output.input.run.context.cancel_filing();
    assert!(
        fx.output
            .input
            .run
            .context
            .take_cancelled_filing()
            .is_none()
    );
    assert!(matches!(
        fx.output
            .input
            .run
            .context
            .take_filing(&mut fx.output.input.run.clock,),
        Err(RunFailure::PairOwned)
    ));
    fx.output
        .input
        .run
        .context
        .bar_pending_pairs(s::NonDispatchCause::Stop, &mut fx.output.input.run.clock)
        .unwrap();
    fx.complete();
    let account = fx
        .output
        .input
        .run
        .context
        .take_repair_accounting(&mut fx.output.input.run.clock)
        .unwrap()
        .unwrap();
    assert_eq!(account.requests()[0].call_id, call);
    let completed = fx
        .output
        .input
        .run
        .context
        .completed_pair(&call, &mut fx.output.input.run.clock)
        .unwrap()
        .unwrap();
    assert_eq!(
        completed.ended().reason,
        Some(s::PairFailure::NotDispatched {
            cause: s::NonDispatchCause::Stop,
        })
    );
    let refused = fx
        .output
        .input
        .run
        .context
        .begin_repair_not_dispatched(
            account,
            s::NonDispatchCause::Stop,
            &mut fx.output.input.run.clock,
        )
        .unwrap_err();
    assert_eq!(refused.refusal, PairBeginRefusal::LostCustody);
    assert!(refused.failure.is_none());
    assert_eq!(refused.account.requests()[0].call_id, call);
    assert_eq!(fx.output.input.records(Kind::ToolCallStarted).len(), 1);
    assert_eq!(fx.output.input.records(Kind::ToolCallCompleted).len(), 1);
    assert!(fx.output.input.run.context.failure().is_none());
}
