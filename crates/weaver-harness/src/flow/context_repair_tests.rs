//! Repair through recorded template checks, whole generations and real sockets.
//! Scripted replies test protocol ownership, not production language qualification.

use super::input_tests::InputFixture;
use super::output_tests::{original, reply, text};
use super::*;
use crate::flow::coordinator::{
    OriginalRequests, RequestScheme, TransportSource, tests::operation,
};
use crate::flow::pump::LifecyclePorts;
use crate::flow::ste100_channel::{Receive, Tag};
use crate::flow::templates::{CompiledInventory, RestatementTemplate, TemplateCertificate};
use sha2::{Digest as _, Sha256};
use std::io::{Read, Write};
use weaver_traits::{ContentBlock, ToolCall};
use weaver_types::state_flow as s;

#[path = "context_repair_custody_tests.rs"]
mod custody_tests;
#[path = "context_empty_correction_tests.rs"]
mod empty_correction_tests;
#[path = "context_recall_refusal_tests.rs"]
mod recall_refusal_tests;
#[path = "context_sort_tests.rs"]
mod sort_tests;

static RESTATEMENT: RestatementTemplate = RestatementTemplate::new("fixture.restatement");

fn ports<I: PumpIo>(io: &mut I) -> PumpPorts<'_, I> {
    PumpPorts {
        lifecycle: LifecyclePorts::default(),
        gate: None,
        decode: None,
        label: None,
        io,
    }
}

fn sequence(record: &serde_json::Value) -> u64 {
    record["sequence"].as_str().unwrap().parse().unwrap()
}

fn call() -> ContentBlock {
    ContentBlock::ToolCall(ToolCall {
        name: "sh".into(),
        arguments: "opaque original command".into(),
    })
}

struct RepairFixture {
    input: InputFixture,
    scheme: RequestScheme,
    certificate: Option<TemplateCertificate>,
    bytes: Vec<u8>,
    filings: Vec<s::StateFrame>,
    answered: Vec<bool>,
    operation: u64,
}

impl RepairFixture {
    fn new() -> Self {
        Self::with_template("Read correction.")
    }

    fn with_template(words: &'static str) -> Self {
        Self::with_outbound_template(words, None)
    }

    fn with_outbound_template(words: &'static str, outbound: Option<&'static str>) -> Self {
        Self::with_sort_partition(words, outbound, true)
    }

    fn with_sort_partition(
        words: &'static str,
        outbound: Option<&'static str>,
        selected: bool,
    ) -> Self {
        let mut entries = vec![s::TemplateSpec {
            template_id: RESTATEMENT.template_id().into(),
            kind: "loop.instruction".into(),
            parts: vec![s::TemplatePart::Literal(words.into())],
        }];
        if let Some(words) = outbound {
            entries.push(s::TemplateSpec {
                template_id: "sort.outbound".into(),
                kind: "sort.decision".into(),
                parts: vec![
                    s::TemplatePart::Literal(words.into()),
                    s::TemplatePart::Slot {
                        name: "order".into(),
                        class: s::SlotClass::TechnicalName,
                    },
                    s::TemplatePart::Literal(".".into()),
                ],
            });
        }
        let templates = Box::leak(entries.into_boxed_slice());
        let inventory = CompiledInventory::new(templates).unwrap();
        let seed: s::SeedRequest = s::decode_strict(include_str!("fixtures/seed.json")).unwrap();
        // Give recall its own key in the actual loaded artifact. Ordinary Read
        // statements must not silently become forbidden correction-pass calls.
        let mut artifact: serde_json::Value = serde_json::from_str(&seed.ste100_scheme).unwrap();
        let mut recall = artifact["verbs"][0].clone();
        recall["key"] = serde_json::json!("recall");
        recall["forms"] = serde_json::json!(["Recall"]);
        artifact["verbs"].as_array_mut().unwrap().push(recall);
        artifact["recall_keys"] = serde_json::json!(["recall"]);
        if outbound.is_some() {
            artifact["technical_names"] = serde_json::json!([{
                "text": "identity", "authority": {"rule_id": "test-rule", "disposition": "technical_use"}
            }]);
        }
        let exact = serde_json::to_string(&artifact).unwrap();
        let digest = s::Digest::new(format!("{:x}", Sha256::digest(exact.as_bytes()))).unwrap();
        let declaration = s::SchemeDeclaration {
            scheme_id: format!("ste100:{digest}"),
            artifact_sha256: digest,
            verbs: vec!["read".into(), "recall".into()],
        };
        let scheme = RequestScheme::from_exact_scheme(&exact, &declaration).unwrap();
        let mut config = super::tests::config();
        if outbound.is_some() && selected {
            let mut rule = config
                .partition
                .iter()
                .find(|rule| rule.kind == "loop.instruction")
                .unwrap()
                .clone();
            rule.id = "outbound".into();
            rule.kind = "sort.decision".into();
            config.partition.push(rule);
        }
        let mut input = InputFixture::with_declaration_scheme_tools(
            Some(&inventory),
            config,
            Some((exact, declaration)),
            None,
        );
        input
            .run
            .context
            .begin_template_checks(inventory, &mut input.run.clock)
            .unwrap();
        let mut certificate = None;
        for _ in 0..128 {
            input
                .run
                .context
                .advance_template_checks(&mut input.run.clock)
                .unwrap();
            input.classifier_transport();
            if let Receive::Complete(message) = input.peer.try_receive().unwrap() {
                assert_eq!(message.tag, Tag::Request);
                let request: s::Ste100Request =
                    s::decode_strict(std::str::from_utf8(&message.payload).unwrap()).unwrap();
                assert!(
                    matches!(&request.target, s::Ste100Target::Template { template_id }
                    if template_id == RESTATEMENT.template_id() || (outbound.is_some() && template_id == "sort.outbound"))
                );
                input.reply(&s::Ste100Reply {
                    request_id: request.request_id,
                    target: request.target,
                    scheme_id: request.scheme_id,
                    result: s::Ste100Result::Accepted { bindings: vec![] },
                });
            }
            certificate = input
                .run
                .context
                .take_template_certificate(&mut input.run.clock)
                .unwrap();
            if certificate.is_some() {
                break;
            }
        }
        assert!(certificate.is_some(), "real template reply never certified");
        input
            .run
            .context
            .complete_wait(PumpDeadline::Enter)
            .unwrap();
        input.run.state.set_nonblocking(true).unwrap();
        input
            .run
            .author(AuthoredEvent {
                kind: Kind::TurnStarted,
                subsystem: weaver_trace::Subsystem::Harness,
                turn: Some(weaver_trace::TurnRef("output-turn".into())),
                source: crate::flow::author::AuthoredSource::Runtime,
                causal_parent: None,
                payload: None,
            })
            .unwrap();
        Self {
            input,
            scheme,
            certificate,
            bytes: vec![],
            filings: vec![],
            answered: vec![],
            operation: 2,
        }
    }

    fn arm(&mut self) {
        if self
            .input
            .run
            .context
            .deadlines
            .get(PumpDeadline::Decision)
            .is_none()
        {
            let now = self.input.run.clock.now().unwrap();
            self.input
                .run
                .context
                .arm(
                    PumpDeadline::Decision,
                    AbsoluteBound::after(now, 10000).unwrap(),
                    &mut self.input.run.clock,
                )
                .unwrap();
        }
    }

    fn transport(&mut self, source: TransportSource) {
        let context = &mut self.input.run.context;
        context
            .flow
            .transport_one(
                &mut FlowRecording {
                    barrier: &mut context.barrier,
                    inbox: &mut context.inbox,
                    budget: &mut context.budget,
                    author: &context.author,
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
        Some(request)
    }

    fn checker(&mut self) {
        if let Some(request) = self.receive_check() {
            let rejected = request.text.starts_with("bad") || request.text.is_empty();
            self.input.reply(&reply(&request, rejected));
        }
    }

    fn member(&mut self, answer: bool) {
        self.transport(TransportSource::State);
        let mut bytes = [0; 65536];
        loop {
            match self.input.run.state.read(&mut bytes) {
                Ok(0) => panic!("member closed during repair"),
                Ok(n) => self.bytes.extend_from_slice(&bytes[..n]),
                Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => break,
                Err(error) => panic!("repair member read: {error}"),
            }
        }
        while let Some(end) = self.bytes.iter().position(|byte| *byte == b'\n') {
            let line: Vec<_> = self.bytes.drain(..=end).collect();
            let request = s::decode_state_line(&line, s::FrameDirection::Request).unwrap();
            assert!(matches!(
                request,
                s::StateFrame::File { .. } | s::StateFrame::FilePair { .. }
            ));
            self.filings.push(request);
            self.answered.push(false);
        }
        if answer {
            for index in 0..self.filings.len() {
                if self.answered[index] {
                    continue;
                }
                let request = &self.filings[index];
                let block_id = match request {
                    s::StateFrame::File { filing, .. } => filing.block_id.clone(),
                    s::StateFrame::FilePair { filing, .. } => filing.block_id.clone(),
                    _ => unreachable!(),
                };
                let began = self.input.run.clock.now().unwrap() + 1;
                let response = s::StateFrame::Filed {
                    common: request.common().clone(),
                    block_id,
                    observation: operation(request, self.operation, began),
                };
                self.operation += 1;
                self.input.run.clock.advance_to(began + 1).unwrap();
                self.input
                    .run
                    .state
                    .write_all(
                        &s::encode_state_line(&response, s::FrameDirection::Response).unwrap(),
                    )
                    .unwrap();
                self.answered[index] = true;
            }
        }
        self.transport(TransportSource::State);
    }

    fn generation(&mut self, content: Vec<ContentBlock>, file: bool) -> OriginalRequests {
        self.arm();
        self.input
            .run
            .context
            .begin_output(original(content), None, &mut self.input.run.clock)
            .unwrap();
        for _ in 0..512 {
            let progress = self
                .input
                .run
                .context
                .advance_output(&mut ports(&mut self.input.run.clock))
                .unwrap();
            self.checker();
            self.member(file);
            if progress == OutputProgress::Complete {
                let checked = self
                    .input
                    .run
                    .context
                    .take_output(&mut self.input.run.clock)
                    .unwrap()
                    .unwrap();
                return OriginalRequests::from_checked(checked, &self.scheme).unwrap();
            }
        }
        panic!("repair fixture generation never completed recorded checks");
    }

    fn begin(&mut self, original: OriginalRequests) {
        self.input
            .run
            .context
            .begin_repair(
                original,
                self.certificate.take().unwrap(),
                &RESTATEMENT,
                &mut self.input.run.clock,
            )
            .unwrap();
    }

    fn step(&mut self, file: bool) -> RepairProgress {
        let progress = self
            .input
            .run
            .context
            .advance_repair(&mut ports(&mut self.input.run.clock))
            .unwrap();
        self.checker();
        self.member(file);
        progress
    }

    fn restatement(&mut self, file: bool) -> repair::RestatementRequest {
        for _ in 0..512 {
            self.step(file);
            if let Some(request) = self
                .input
                .run
                .context
                .take_restatement(&mut self.input.run.clock)
                .unwrap()
            {
                return request;
            }
        }
        panic!("repair did not record and transfer its restatement request");
    }

    fn observe(&mut self, source: &s::SourceBlockId, correction: OriginalRequests) {
        self.input
            .run
            .context
            .observe_correction(source, correction, &mut self.input.run.clock)
            .unwrap();
    }

    fn effective(&mut self, file: bool) -> repair::EffectiveEmission {
        for _ in 0..512 {
            self.step(file);
            if let Some(effective) = self
                .input
                .run
                .context
                .take_effective_emission(&mut self.input.run.clock)
                .unwrap()
            {
                return effective;
            }
        }
        panic!("repair did not release the complete effective emission");
    }

    fn repair_records(&self, phase: &str) -> Vec<serde_json::Value> {
        self.input
            .records(Kind::StateFlow)
            .into_iter()
            .filter(|record| record["payload"]["phase"] == phase)
            .collect()
    }
}

#[test]
fn accepted_generation_needs_no_restatement_or_ordinary_filing_receipt() {
    let mut fx = RepairFixture::new();
    let original = fx.generation(vec![text("Read first."), text("Read second.")], false);
    let source = original.generation().source().clone();
    fx.begin(original);
    let effective = fx.effective(false);
    assert_eq!(effective.source(), &source);
    assert_eq!(effective.text(), "Read first.\nRead second.");
    assert_eq!(effective.statements().len(), 2);
    assert!(!effective.attempted_call());
    assert!(effective.requests().is_empty());
    assert!(fx.input.records(Kind::LoopInstruction).is_empty());
    assert!(fx.repair_records("repair_requested").is_empty());
    assert!(fx.answered.iter().all(|answer| !answer));
    assert!(fx.input.run.context.flow.ledger().facts().next().is_some());
}

#[test]
fn recorded_restatement_preserves_siblings_and_multiblock_correction_without_filing_wait() {
    let mut fx = RepairFixture::new();
    let original = fx.generation(
        vec![
            text("Read before."),
            text("bad original"),
            text("Read after."),
        ],
        false,
    );
    let source = original.generation().source().clone();
    fx.begin(original);
    assert!(
        fx.input
            .run
            .context
            .take_restatement(&mut fx.input.run.clock)
            .unwrap()
            .is_none()
    );
    let request = fx.restatement(false);
    assert_eq!(request.source().sequence, source.block_id.sequence);
    assert_eq!(request.source().block.0, 1);
    assert_eq!(request.attempt().0, 1);
    assert_eq!(request.instruction().text(), "Read correction.");
    let records = fx.input.records(Kind::LoopInstruction);
    assert_eq!(records.len(), 1);
    let record = &records[0];
    assert_eq!(record["payload"]["template_id"], RESTATEMENT.template_id());
    assert_eq!(
        record["payload"]["message"],
        serde_json::to_value(request.instruction().message()).unwrap()
    );
    assert_eq!(
        record["payload"]["instruction"]["restatement"]["source"],
        serde_json::to_value(request.source()).unwrap()
    );
    assert_eq!(sequence(record), request.recorded().block_id.sequence.0);
    let requested = fx.repair_records("repair_requested");
    assert_eq!(requested.len(), 1);
    assert!(sequence(&requested[0]) < sequence(record));
    assert!(
        fx.input
            .run
            .context
            .take_restatement(&mut fx.input.run.clock)
            .unwrap()
            .is_none()
    );

    let correction = fx.generation(
        vec![text("Read correction one."), text("Read correction two.")],
        false,
    );
    let correction_source = correction.generation().source().clone();
    fx.observe(request.source(), correction);
    let effective = fx.effective(false);
    assert_eq!(effective.source(), &source);
    assert_eq!(
        effective.text(),
        "Read before.\nRead correction one.\nRead correction two.\nRead after."
    );
    let sources: Vec<_> = effective
        .statements()
        .iter()
        .map(|statement| (statement.block_id.sequence, statement.block_id.block.0))
        .collect();
    assert_eq!(
        sources,
        vec![
            (source.block_id.sequence, 0),
            (correction_source.block_id.sequence, 0),
            (correction_source.block_id.sequence, 1),
            (source.block_id.sequence, 2)
        ]
    );
    assert!(fx.answered.iter().all(|answer| !answer));
    assert!(fx.input.run.context.flow.ledger().facts().next().is_some());
    assert!(
        fx.input
            .run
            .context
            .take_effective_emission(&mut fx.input.run.clock)
            .unwrap()
            .is_none()
    );
}

#[test]
fn multiple_rejected_blocks_count_once_per_correction_and_cap_is_recorded_once() {
    let mut fx = RepairFixture::new();
    let original = fx.generation(vec![text("bad original")], true);
    fx.begin(original);
    let first = fx.restatement(true);
    let correction = fx.generation(vec![text("bad one"), text("bad two")], true);
    fx.observe(first.source(), correction);
    let second = fx.restatement(true);
    assert_eq!(second.source(), first.source());
    assert_eq!(second.attempt().0, 2);
    assert_eq!(fx.repair_records("repair_requested").len(), 1);
    assert!(fx.repair_records("repair_exhausted").is_empty());
    let correction = fx.generation(vec![text("bad three"), text("bad four")], true);
    fx.observe(second.source(), correction);
    let mut exhausted = None;
    for _ in 0..512 {
        fx.step(true);
        exhausted = fx
            .input
            .run
            .context
            .take_repair_exhausted(&mut fx.input.run.clock)
            .unwrap();
        if exhausted.is_some() {
            break;
        }
    }
    let exhausted = exhausted.expect("cap must terminate the finite repair");
    assert_eq!(exhausted.source(), first.source());
    assert_eq!(exhausted.failed_corrections().0, 2);
    assert_eq!(exhausted.retry_cap().0, 2);
    assert_eq!(fx.repair_records("repair_requested").len(), 1);
    let caps = fx.repair_records("repair_exhausted");
    assert_eq!(caps.len(), 1);
    assert_eq!(caps[0]["payload"]["failed_corrections"], "2");
    assert_eq!(fx.input.records(Kind::LoopInstruction).len(), 2);
    assert!(
        fx.input
            .run
            .context
            .take_effective_emission(&mut fx.input.run.clock)
            .unwrap()
            .is_none()
    );
    assert!(fx.input.run.context.failure().is_none());
}

#[test]
fn rejected_original_blocks_are_repaired_in_original_index_order() {
    let mut fx = RepairFixture::new();
    let original = fx.generation(
        vec![text("bad first"), text("Read middle."), text("bad last")],
        false,
    );
    fx.begin(original);
    let first = fx.restatement(false);
    assert_eq!(first.source().block.0, 0);
    let correction = fx.generation(vec![text("Read first replacement.")], false);
    fx.observe(first.source(), correction);
    let last = fx.restatement(false);
    assert_eq!(last.source().sequence, first.source().sequence);
    assert_eq!(last.source().block.0, 2);
    assert_eq!(last.attempt().0, 1);
    assert!(last.recorded().block_id.sequence > first.recorded().block_id.sequence);
    let correction = fx.generation(vec![text("Read last replacement.")], false);
    fx.observe(last.source(), correction);
    let effective = fx.effective(false);
    assert_eq!(
        effective.text(),
        "Read first replacement.\nRead middle.\nRead last replacement."
    );
    let requested = fx.repair_records("repair_requested");
    assert_eq!(requested.len(), 2);
    assert_eq!(
        requested[0]["payload"]["source_id"],
        serde_json::to_value(first.source()).unwrap()
    );
    assert_eq!(
        requested[1]["payload"]["source_id"],
        serde_json::to_value(last.source()).unwrap()
    );
    assert!(fx.repair_records("repair_exhausted").is_empty());
}

#[test]
fn correction_call_commits_pass_purpose_pair_before_a_later_success() {
    let mut fx = RepairFixture::new();
    let original = fx.generation(vec![text("bad original")], true);
    fx.begin(original);
    let first = fx.restatement(true);
    let correction = fx.generation(vec![text("Read forbidden."), call()], false);
    let call_id = correction.requests()[0].opening().call_id.clone();
    fx.observe(first.source(), correction);
    for _ in 0..16 {
        fx.step(false);
        assert!(
            fx.input
                .run
                .context
                .take_restatement(&mut fx.input.run.clock)
                .unwrap()
                .is_none()
        );
    }
    let second = fx.restatement(true);
    assert_eq!(second.attempt().0, 2);
    let completed = fx
        .input
        .run
        .context
        .completed_pair(&call_id, &mut fx.input.run.clock)
        .unwrap()
        .expect("forbidden correction pair must have its actual terminal receipt");
    assert_eq!(completed.ended().fate, s::TerminalPairFate::Failed);
    assert!(matches!(
        completed.ended().reason,
        Some(s::PairFailure::NotDispatched {
            cause: s::NonDispatchCause::PassPurpose
        })
    ));
    let correction = fx.generation(vec![text("Read replacement.")], true);
    fx.observe(second.source(), correction);
    let effective = fx.effective(true);
    assert_eq!(effective.text(), "Read replacement.");
    assert!(effective.requests().is_empty());
    assert!(!effective.attempted_call());
    assert_eq!(fx.input.records(Kind::ToolCallStarted).len(), 1);
    assert_eq!(fx.input.records(Kind::ToolCallCompleted).len(), 1);
    assert!(fx.input.run.context.failure().is_none());
}

#[test]
fn exhausted_repair_accounts_original_call_before_returning_its_refusal() {
    let mut fx = RepairFixture::new();
    let original = fx.generation(
        vec![text("Read held request."), call(), text("bad original")],
        false,
    );
    let call_id = original.requests()[0].opening().call_id.clone();
    fx.begin(original);
    let first = fx.restatement(true);
    let correction = fx.generation(vec![text("bad correction one")], true);
    fx.observe(first.source(), correction);
    let second = fx.restatement(true);
    let correction = fx.generation(vec![text("bad correction two")], true);
    fx.observe(second.source(), correction);
    for _ in 0..16 {
        fx.step(false);
        assert!(
            fx.input
                .run
                .context
                .take_repair_exhausted(&mut fx.input.run.clock)
                .unwrap()
                .is_none()
        );
    }
    let mut exhausted = None;
    for _ in 0..512 {
        fx.step(true);
        exhausted = fx
            .input
            .run
            .context
            .take_repair_exhausted(&mut fx.input.run.clock)
            .unwrap();
        if exhausted.is_some() {
            break;
        }
    }
    let exhausted = exhausted.expect("original request accounting must finish the cap outcome");
    assert_eq!(exhausted.failed_corrections().0, 2);
    let completed = fx
        .input
        .run
        .context
        .completed_pair(&call_id, &mut fx.input.run.clock)
        .unwrap()
        .unwrap();
    assert!(matches!(
        completed.ended().reason,
        Some(s::PairFailure::NotDispatched {
            cause: s::NonDispatchCause::RepairExhausted
        })
    ));
    assert_eq!(fx.input.records(Kind::ToolCallStarted).len(), 1);
    assert_eq!(fx.input.records(Kind::ToolCallCompleted).len(), 1);
    assert!(
        fx.input
            .run
            .context
            .take_effective_emission(&mut fx.input.run.clock)
            .unwrap()
            .is_none()
    );
    assert!(fx.input.run.context.failure().is_none());
}

#[test]
fn recorder_pressure_cannot_grant_an_unrecorded_restatement_or_duplicate_it() {
    let mut fx = RepairFixture::new();
    let original = fx.generation(vec![text("bad original")], true);
    fx.begin(original);
    fx.input.fill_trace();
    for _ in 0..4 {
        fx.step(false);
        assert!(
            fx.input
                .run
                .context
                .take_restatement(&mut fx.input.run.clock)
                .unwrap()
                .is_none()
        );
        assert!(fx.input.records(Kind::LoopInstruction).is_empty());
    }
    fx.input.drain_trace();
    let request = fx.restatement(true);
    assert_eq!(request.attempt().0, 1);
    assert_eq!(fx.repair_records("repair_requested").len(), 1);
    assert_eq!(fx.input.records(Kind::LoopInstruction).len(), 1);
    assert!(
        fx.input
            .run
            .context
            .take_restatement(&mut fx.input.run.clock)
            .unwrap()
            .is_none()
    );
}

#[test]
fn repair_refuses_foreign_inventory_certificate_and_wrong_rejected_source() {
    let mut foreign = RepairFixture::with_template("Read foreign.");
    let certificate = foreign.certificate.take().unwrap();
    let mut fx = RepairFixture::new();
    let original = fx.generation(vec![text("bad original")], true);
    assert!(
        fx.input
            .run
            .context
            .begin_repair(original, certificate, &RESTATEMENT, &mut fx.input.run.clock)
            .is_err()
    );
    assert!(fx.input.run.context.failure().is_none());
    assert!(fx.input.records(Kind::LoopInstruction).is_empty());

    let mut fx = RepairFixture::new();
    let original = fx.generation(vec![text("bad original")], true);
    fx.begin(original);
    let request = fx.restatement(true);
    let correction = fx.generation(vec![text("Read correction.")], true);
    let mut wrong = request.source().clone();
    wrong.block = (wrong.block.0 + 1).into();
    assert!(
        fx.input
            .run
            .context
            .observe_correction(&wrong, correction, &mut fx.input.run.clock)
            .is_err()
    );
    assert!(fx.input.run.context.failure().is_none());
    assert!(
        fx.input
            .run
            .context
            .take_effective_emission(&mut fx.input.run.clock)
            .unwrap()
            .is_none()
    );
}

#[test]
fn a_structural_repair_refusal_records_its_original_attribution_and_case() {
    static UNKNOWN: RestatementTemplate = RestatementTemplate::new("fixture.unknown");
    for unknown_family in [false, true] {
        let mut fx = RepairFixture::new();
        let original = fx.generation(vec![text("bad original")], true);
        let source = original.generation().source().clone();
        let certificate = if unknown_family {
            fx.certificate.take().unwrap()
        } else {
            RepairFixture::with_template("Read foreign.")
                .certificate
                .take()
                .unwrap()
        };
        let template = if unknown_family {
            &UNKNOWN
        } else {
            &RESTATEMENT
        };
        let refusal = fx
            .input
            .run
            .context
            .begin_repair(original, certificate, template, &mut fx.input.run.clock)
            .unwrap_err();
        assert!(
            fx.input.records(Kind::Refusal).is_empty(),
            "the returned refusal is owed, not yet recorded"
        );
        let (_original, _certificate, event) = refusal.into_parts();
        let recorded = fx
            .input
            .run
            .context
            .author_and_wait(
                &mut ports(&mut fx.input.run.clock),
                event.expect("structural refusal must retain its exact authored event"),
            )
            .unwrap();
        let records = fx.input.records(Kind::Refusal);
        assert_eq!(records.len(), 1);
        assert_eq!(sequence(&records[0]), recorded.0);
        assert_eq!(records[0]["turn"], source.turn.unwrap());
        assert_eq!(
            records[0]["causal_parent"],
            source.block_id.sequence.0.to_string()
        );
        assert_eq!(records[0]["payload"]["seam"], "instruction");
        assert_eq!(records[0]["payload"]["template_id"], template.template_id());
        assert_eq!(
            records[0]["payload"]["case"],
            if unknown_family {
                "invalid_template"
            } else {
                "invalid_context"
            }
        );
        assert!(fx.input.records(Kind::LoopInstruction).is_empty());
        assert!(fx.input.run.context.failure().is_none());
        assert!(
            fx.input
                .run
                .context
                .take_restatement(&mut fx.input.run.clock)
                .unwrap()
                .is_none()
        );
        assert!(
            fx.input
                .run
                .context
                .take_effective_emission(&mut fx.input.run.clock)
                .unwrap()
                .is_none()
        );
    }
}

#[test]
fn a_completed_repair_returns_the_one_certificate_for_the_next_generation() {
    let mut fx = RepairFixture::new();
    let inventory = fx.certificate.as_ref().unwrap().inventory_sha256().clone();
    let original = fx.generation(vec![text("Read first.")], true);
    fx.begin(original);
    assert!(
        fx.input
            .run
            .context
            .take_repair_certificate(&mut fx.input.run.clock)
            .unwrap()
            .is_none()
    );
    assert_eq!(fx.effective(true).text(), "Read first.");
    let certificate = fx
        .input
        .run
        .context
        .take_repair_certificate(&mut fx.input.run.clock)
        .unwrap()
        .expect("completed owner must return its certificate");
    assert_eq!(certificate.inventory_sha256(), &inventory);
    assert!(
        fx.input
            .run
            .context
            .take_repair_certificate(&mut fx.input.run.clock)
            .unwrap()
            .is_none()
    );
    fx.certificate = Some(certificate);
    let original = fx.generation(vec![text("bad next generation")], true);
    fx.begin(original);
    let request = fx.restatement(true);
    let correction = fx.generation(vec![text("Read second.")], true);
    fx.observe(request.source(), correction);
    assert_eq!(fx.effective(true).text(), "Read second.");
    assert_eq!(fx.input.records(Kind::LoopInstruction).len(), 1);
    assert!(
        fx.input
            .run
            .context
            .take_repair_certificate(&mut fx.input.run.clock)
            .unwrap()
            .is_some()
    );
    assert!(fx.input.run.context.failure().is_none());
}

#[test]
fn correction_classifier_component_failure_halts_without_repair_success() {
    let mut fx = RepairFixture::new();
    let generation = fx.generation(vec![text("bad original")], true);
    fx.begin(generation);
    let _request = fx.restatement(true);
    fx.arm();
    fx.input
        .run
        .context
        .begin_output(
            original(vec![text("Read pending.")]),
            None,
            &mut fx.input.run.clock,
        )
        .unwrap();
    let mut request = None;
    for _ in 0..256 {
        fx.input
            .run
            .context
            .advance_output(&mut ports(&mut fx.input.run.clock))
            .unwrap();
        if let Some(received) = fx.receive_check() {
            if received.text == "Read pending." {
                request = Some(received);
            } else {
                fx.input.reply(&reply(&received, false));
            }
        }
        fx.member(true);
        if request.is_some() {
            break;
        }
    }
    let request = request.expect("correction check must reach the actual socket");
    fx.input.reply(&s::Ste100Reply {
        request_id: request.request_id,
        target: request.target,
        scheme_id: request.scheme_id,
        result: s::Ste100Result::ComponentFailed {
            code: s::Ste100ComponentCode::Crashed,
            detail: "scripted component crash".into(),
        },
    });
    let mut failed = false;
    for _ in 0..256 {
        if fx
            .input
            .run
            .context
            .advance_output(&mut ports(&mut fx.input.run.clock))
            .is_err()
        {
            failed = true;
            break;
        }
        fx.member(true);
    }
    assert!(failed);
    assert!(fx.input.run.context.failure().is_some());
    assert!(
        fx.input
            .run
            .context
            .take_effective_emission(&mut fx.input.run.clock)
            .is_err()
    );
    assert!(
        fx.input
            .run
            .context
            .take_restatement(&mut fx.input.run.clock)
            .is_err()
    );
}

#[test]
fn a_selected_runtime_instruction_defect_refuses_repair_without_a_halt_or_success() {
    let mut fx = RepairFixture::new();
    let original = fx.generation(
        vec![text("Read held sibling."), text("bad original")],
        false,
    );
    fx.begin(original);
    let mut rejected = false;
    let mut refused = false;
    for _ in 0..512 {
        let progress = fx
            .input
            .run
            .context
            .advance_repair(&mut ports(&mut fx.input.run.clock))
            .unwrap();
        if let Some(request) = fx.receive_check() {
            let is_instruction = request.text == "Read correction.";
            rejected |= is_instruction;
            fx.input.reply(&reply(
                &request,
                is_instruction || request.text.starts_with("bad"),
            ));
        }
        fx.member(false);
        if progress == RepairProgress::Refused {
            refused = true;
            break;
        }
    }
    assert!(
        rejected,
        "the selected instruction must reach the real checker"
    );
    assert!(
        refused,
        "correct template rejection must finish as an operation refusal"
    );
    assert!(fx.input.run.context.repair_refused());
    assert!(fx.input.run.context.failure().is_none());
    assert!(fx.input.run.context.halt().is_none());
    assert!(
        fx.input
            .run
            .context
            .take_restatement(&mut fx.input.run.clock)
            .unwrap()
            .is_none()
    );
    assert!(
        fx.input
            .run
            .context
            .take_effective_emission(&mut fx.input.run.clock)
            .unwrap()
            .is_none()
    );
    assert!(
        fx.input
            .run
            .context
            .take_repair_exhausted(&mut fx.input.run.clock)
            .unwrap()
            .is_none()
    );
    let faults = fx.input.records(Kind::Fault);
    assert_eq!(faults.len(), 1);
    assert_eq!(faults[0]["payload"]["case"], "state_template_rejected");
    assert_eq!(fx.input.records(Kind::LoopInstruction).len(), 1);
    assert_eq!(fx.input.records(Kind::MessageAssistant).len(), 1);
    assert!(fx.answered.iter().all(|answer| !answer));
    assert!(fx.input.run.context.flow.ledger().facts().next().is_some());
    let refused = fx
        .input
        .run
        .context
        .take_refused_repair(&mut fx.input.run.clock)
        .unwrap()
        .expect("recorded refusal must return retained custody");
    assert_eq!(
        refused
            .account()
            .original()
            .generation()
            .original()
            .message
            .content
            .len(),
        2
    );
    assert!(refused.pending().is_none());
    let RepairRefusalEvidence::RuntimeTemplate { fault, check } = refused.evidence() else {
        panic!("runtime defect must retain its recorded classifier proof");
    };
    assert_eq!(fault.block_id.sequence.0, sequence(&faults[0]));
    assert!(matches!(
        check.reply().result,
        s::Ste100Result::NotSte100 { .. }
    ));
    assert!(check.source().block_id.sequence < fault.block_id.sequence);
    assert!(
        fx.input
            .run
            .context
            .take_refused_repair(&mut fx.input.run.clock)
            .unwrap()
            .is_none()
    );
    assert!(
        fx.input
            .run
            .context
            .take_repair_certificate(&mut fx.input.run.clock)
            .unwrap()
            .is_some()
    );
}

#[test]
fn cancellation_preserves_the_selected_instruction_rejection_record() {
    let mut fx = RepairFixture::new();
    let original = fx.generation(vec![text("Read retained."), text("bad original")], false);
    fx.begin(original);
    let mut instruction = None;
    for _ in 0..512 {
        fx.input
            .run
            .context
            .advance_repair(&mut ports(&mut fx.input.run.clock))
            .unwrap();
        if let Some(request) = fx.receive_check() {
            if request.text == "Read correction." {
                instruction = Some(request);
                break;
            }
            fx.input
                .reply(&reply(&request, request.text.starts_with("bad")));
        }
        fx.member(false);
    }
    let instruction = instruction.expect("selected instruction check must be in flight");
    fx.input
        .run
        .context
        .cancel_repair(s::NonDispatchCause::Stop, &mut fx.input.run.clock)
        .unwrap();
    assert!(
        fx.input
            .run
            .context
            .take_cancelled_repair(&mut fx.input.run.clock)
            .unwrap()
            .is_none()
    );
    assert!(
        fx.input
            .run
            .context
            .take_restatement(&mut fx.input.run.clock)
            .unwrap()
            .is_none()
    );
    fx.input.reply(&reply(&instruction, true));
    let mut cancelled = None;
    for _ in 0..512 {
        fx.step(false);
        cancelled = fx
            .input
            .run
            .context
            .take_cancelled_repair(&mut fx.input.run.clock)
            .unwrap();
        if cancelled.is_some() {
            break;
        }
    }
    assert!(cancelled.is_some());
    let faults = fx.input.records(Kind::Fault);
    assert_eq!(faults.len(), 1);
    assert_eq!(faults[0]["payload"]["case"], "state_template_rejected");
    let result_source = &faults[0]["payload"]["account"]["result_source"]["block_id"];
    assert!(
        fx.input
            .records(Kind::Ste100Result)
            .iter()
            .any(|record| record["sequence"] == result_source["sequence"]
                && record["payload"]["result"].get("not_ste100").is_some())
    );
    assert!(
        fx.input
            .run
            .context
            .take_restatement(&mut fx.input.run.clock)
            .unwrap()
            .is_none()
    );
    assert!(
        fx.input
            .run
            .context
            .take_effective_emission(&mut fx.input.run.clock)
            .unwrap()
            .is_none()
    );
    assert!(fx.input.run.context.failure().is_none());
    assert!(fx.answered.iter().all(|answer| !answer));
}

#[test]
fn cancelling_before_or_after_a_restatement_grant_returns_custody_without_success() {
    for after_grant in [false, true] {
        let mut fx = RepairFixture::new();
        let original = fx.generation(vec![text("Read retained."), text("bad original")], false);
        let source = original.generation().source().clone();
        fx.begin(original);
        if after_grant {
            let request = fx.restatement(false);
            assert_eq!(request.source().sequence, source.block_id.sequence);
        }
        fx.input
            .run
            .context
            .cancel_repair(s::NonDispatchCause::Stop, &mut fx.input.run.clock)
            .unwrap();
        let mut cancelled = None;
        for _ in 0..512 {
            fx.step(false);
            cancelled = fx
                .input
                .run
                .context
                .take_cancelled_repair(&mut fx.input.run.clock)
                .unwrap();
            if cancelled.is_some() {
                break;
            }
        }
        let cancelled = cancelled.expect("cancellation must return retained custody");
        assert_eq!(cancelled.account().source(), &source);
        assert!(
            fx.input
                .run
                .context
                .take_restatement(&mut fx.input.run.clock)
                .unwrap()
                .is_none()
        );
        assert!(
            fx.input
                .run
                .context
                .take_effective_emission(&mut fx.input.run.clock)
                .unwrap()
                .is_none()
        );
        assert!(
            fx.input
                .run
                .context
                .take_repair_exhausted(&mut fx.input.run.clock)
                .unwrap()
                .is_none()
        );
        assert!(
            fx.input
                .run
                .context
                .take_cancelled_repair(&mut fx.input.run.clock)
                .unwrap()
                .is_none()
        );
        assert_eq!(
            fx.input.records(Kind::LoopInstruction).len(),
            usize::from(after_grant)
        );
        assert!(fx.answered.iter().all(|answer| !answer));
        assert!(fx.input.run.context.flow.ledger().facts().next().is_some());
        assert!(fx.input.run.context.failure().is_none());
    }
}

#[test]
fn generic_author_and_resume_cannot_steal_the_pending_repair_record() {
    let mut fx = RepairFixture::new();
    let original = fx.generation(vec![text("bad original")], true);
    fx.begin(original);
    fx.input.fill_trace();
    for _ in 0..8 {
        fx.step(false);
        if fx.input.run.context.pending_author().is_some() {
            break;
        }
    }
    let ticket = fx
        .input
        .run
        .context
        .pending_author()
        .expect("repair author must wait on actual recorder pressure")
        .ticket;
    let event = super::tests::event(
        Kind::ModelMeasurement,
        Some(Payload::ModelMeasurement(
            serde_json::value::to_raw_value(&serde_json::json!({"fixture":"unrelated"})).unwrap(),
        )),
    );
    let Some(Payload::ModelMeasurement(raw)) = event.payload.as_ref() else {
        unreachable!()
    };
    let original_bytes = raw.get().to_owned();
    let original_pointer = raw.get().as_ptr();
    assert!(matches!(
        fx.input
            .run
            .context
            .author_and_wait(&mut ports(&mut fx.input.run.clock), event),
        Err(RunFailure::RepairOwned)
    ));
    assert!(matches!(
        fx.input
            .run
            .context
            .resume_author_and_wait(&mut ports(&mut fx.input.run.clock)),
        Err(RunFailure::RepairOwned)
    ));
    assert_eq!(
        fx.input.run.context.pending_author().unwrap().ticket,
        ticket
    );
    assert!(fx.input.run.context.failure().is_none());
    fx.input.drain_trace();
    let request = fx.restatement(true);
    // The admitted repair progresses with its competing caller still held.
    // A new correction output requires the caller to take that exact input.
    let held = fx.input.run.context.take_held_author().unwrap();
    assert_eq!(held.kind, Kind::ModelMeasurement);
    let Some(Payload::ModelMeasurement(raw)) = held.payload else {
        panic!("the held unrelated original must remain unchanged");
    };
    assert_eq!(raw.get(), original_bytes);
    assert_eq!(raw.get().as_ptr(), original_pointer);
    let correction = fx.generation(vec![text("Read replacement.")], true);
    fx.observe(request.source(), correction);
    assert_eq!(fx.effective(true).text(), "Read replacement.");
    assert_eq!(fx.repair_records("repair_requested").len(), 1);
    assert_eq!(fx.input.records(Kind::LoopInstruction).len(), 1);
}

#[test]
fn repair_instruction_resumes_after_leave_interrupts_initial_author_admission() {
    struct AdmissionClock<'a> {
        clock: &'a mut crate::flow::clock::testing::FakePumpIo,
        checked: bool,
        deadline: u64,
    }
    impl PumpIo for AdmissionClock<'_> {
        fn now(&mut self) -> std::io::Result<u64> {
            if self.checked {
                self.clock.advance_to(self.deadline)?;
            }
            self.checked = true;
            self.clock.now()
        }
        fn wait(
            &mut self,
            _: &[crate::flow::clock::WaitInterest<'_>],
            _: Option<u64>,
        ) -> std::io::Result<crate::flow::clock::WaitOutcome> {
            panic!("instruction admission has not submitted a trace ticket");
        }
    }

    let mut fx = RepairFixture::new();
    let original = fx.generation(vec![text("bad original")], true);
    let source = original.generation().source().clone();
    fx.begin(original);
    for _ in 0..128 {
        fx.step(false);
        if fx.repair_records("repair_requested").len() == 1 {
            break;
        }
    }
    assert_eq!(fx.repair_records("repair_requested").len(), 1);
    // The recorded repair request has advanced the ledger. Stage its certified
    // instruction; the following step takes that event into the shared author.
    fx.step(false);
    assert!(fx.input.records(Kind::LoopInstruction).is_empty());
    assert!(fx.input.run.context.waiting_content.is_none());
    let decision = fx
        .input
        .run
        .context
        .deadlines
        .get(PumpDeadline::Decision)
        .unwrap();
    let leave = AbsoluteBound::after(fx.input.run.clock.now().unwrap(), 10).unwrap();
    fx.input
        .run
        .context
        .arm(PumpDeadline::Leave, leave, &mut fx.input.run.clock)
        .unwrap();
    // The repair entry check passes. Leave expires after the repair owner has
    // taken its original and marked author.started, at nested author admission.
    let mut clock = AdmissionClock {
        clock: &mut fx.input.run.clock,
        checked: false,
        deadline: leave.deadline_ns(),
    };
    assert_eq!(
        fx.input
            .run
            .context
            .advance_repair(&mut ports(&mut clock))
            .unwrap_err(),
        RunFailure::Wait {
            key: PumpDeadline::Leave,
            bound: leave
        }
    );
    assert!(fx.input.run.context.author_admission_pending);
    let kept = fx.input.run.context.waiting_content.as_ref().unwrap();
    assert_eq!(kept.kind, Kind::LoopInstruction);
    assert_eq!(
        kept.causal_parent,
        Some(Sequence(source.block_id.sequence.0))
    );
    let Some(Payload::LoopInstruction(instruction)) = &kept.payload else {
        panic!("the deferred owner must keep the exact certified instruction");
    };
    let payload = serde_json::to_value(instruction).unwrap();
    assert!(fx.input.run.context.pending_author().is_none());
    assert!(fx.input.run.context.refused_author().is_none());
    assert!(fx.input.records(Kind::LoopInstruction).is_empty());
    assert!(fx.input.run.context.halt().is_none());
    assert_eq!(
        fx.input.run.context.deadlines.get(PumpDeadline::Decision),
        Some(decision)
    );
    assert_eq!(
        fx.input.run.context.complete_wait(PumpDeadline::Leave),
        Some(leave)
    );

    let request = fx.restatement(true);
    assert_eq!(request.source(), &source.block_id);
    assert_eq!(request.instruction().text(), "Read correction.");
    let records = fx.input.records(Kind::LoopInstruction);
    assert_eq!(records.len(), 1);
    assert_eq!(records[0]["payload"], payload);
    assert_eq!(
        sequence(&records[0]),
        request.recorded().block_id.sequence.0
    );
    assert!(!fx.input.run.context.author_admission_pending);
    assert!(fx.input.run.context.waiting_content.is_none());
    assert!(fx.input.run.context.refused_author().is_none());
    let correction = fx.generation(vec![text("Read replacement.")], true);
    fx.observe(request.source(), correction);
    assert_eq!(fx.effective(true).text(), "Read replacement.");
    assert_eq!(fx.input.records(Kind::LoopInstruction).len(), 1);
    assert_eq!(fx.repair_records("repair_requested").len(), 1);
    assert!(fx.input.run.context.halt().is_none());
}

#[test]
fn cancellation_resumes_the_same_pending_instruction_without_releasing_its_grant() {
    let mut fx = RepairFixture::new();
    let original = fx.generation(vec![text("bad original")], true);
    fx.begin(original);
    for _ in 0..128 {
        fx.step(false);
        if fx.repair_records("repair_requested").len() == 1 {
            break;
        }
    }
    assert_eq!(fx.repair_records("repair_requested").len(), 1);
    assert!(fx.input.records(Kind::LoopInstruction).is_empty());
    fx.input.fill_trace();
    // The generic author waits inside its recorded-ticket call. Interrupt that
    // wait with the independent lifecycle deadline, retaining the same Decision.
    let decision = fx
        .input
        .run
        .context
        .deadlines
        .get(PumpDeadline::Decision)
        .unwrap();
    let leave = AbsoluteBound::after(fx.input.run.clock.now().unwrap(), 5).unwrap();
    fx.input
        .run
        .context
        .arm(PumpDeadline::Leave, leave, &mut fx.input.run.clock)
        .unwrap();
    let mut interrupted = false;
    for _ in 0..128 {
        match fx
            .input
            .run
            .context
            .advance_repair(&mut ports(&mut fx.input.run.clock))
        {
            Ok(_) => {}
            Err(RunFailure::Wait {
                key: PumpDeadline::Leave,
                bound,
            }) => {
                assert_eq!(bound, leave);
                interrupted = true;
                break;
            }
            Err(error) => panic!("unexpected pressure failure: {error:?}"),
        }
        fx.checker();
        fx.member(false);
    }
    assert!(interrupted);
    assert_eq!(
        fx.input.run.context.complete_wait(PumpDeadline::Leave),
        Some(leave)
    );
    assert_eq!(
        fx.input.run.context.deadlines.get(PumpDeadline::Decision),
        Some(decision)
    );
    let pending = fx
        .input
        .run
        .context
        .pending_author()
        .expect("instruction must own a real pending ticket");
    assert_eq!(pending.event.kind, Kind::LoopInstruction);
    let ticket = pending.ticket;
    fx.input
        .run
        .context
        .cancel_repair(s::NonDispatchCause::Unload, &mut fx.input.run.clock)
        .unwrap();
    assert!(
        fx.input
            .run
            .context
            .take_cancelled_repair(&mut fx.input.run.clock)
            .unwrap()
            .is_none()
    );
    assert!(
        fx.input
            .run
            .context
            .take_restatement(&mut fx.input.run.clock)
            .unwrap()
            .is_none()
    );
    assert_eq!(
        fx.input.run.context.pending_author().unwrap().ticket,
        ticket
    );
    fx.input.drain_trace();
    let mut cancelled = None;
    for _ in 0..512 {
        fx.step(true);
        cancelled = fx
            .input
            .run
            .context
            .take_cancelled_repair(&mut fx.input.run.clock)
            .unwrap();
        if cancelled.is_some() {
            break;
        }
    }
    assert!(cancelled.is_some());
    assert_eq!(fx.input.records(Kind::LoopInstruction).len(), 1);
    assert_eq!(fx.repair_records("repair_requested").len(), 1);
    assert!(
        fx.input
            .run
            .context
            .take_restatement(&mut fx.input.run.clock)
            .unwrap()
            .is_none()
    );
    assert!(
        fx.input
            .run
            .context
            .take_effective_emission(&mut fx.input.run.clock)
            .unwrap()
            .is_none()
    );
    assert!(fx.input.run.context.failure().is_none());
}

#[test]
fn cancellation_of_entered_correction_pairs_keeps_the_owned_receipt_path() {
    let mut fx = RepairFixture::new();
    let original = fx.generation(vec![text("bad original")], true);
    fx.begin(original);
    let request = fx.restatement(true);
    let correction = fx.generation(vec![text("Read forbidden."), call()], false);
    let call_id = correction.requests()[0].opening().call_id.clone();
    fx.observe(request.source(), correction);
    let mut opened = false;
    for _ in 0..512 {
        // Allow the statement and instruction to commit, then hold the opening.
        fx.step(false);
        if fx.filings.iter().zip(&fx.answered).any(|(frame, answered)| !answered && matches!(frame, s::StateFrame::FilePair { filing, .. } if matches!(filing.pair, s::PairRecord::Started(_)))) {
            opened = true;
            break;
        }
        if fx
            .filings
            .iter()
            .zip(&fx.answered)
            .any(|(frame, answered)| !answered && matches!(frame, s::StateFrame::File { .. }))
        {
            fx.member(true);
        }
    }
    assert!(
        opened,
        "pair opening must reach the member before cancellation"
    );
    fx.input
        .run
        .context
        .cancel_repair(s::NonDispatchCause::Stop, &mut fx.input.run.clock)
        .unwrap();
    assert!(
        fx.input
            .run
            .context
            .take_cancelled_repair(&mut fx.input.run.clock)
            .unwrap()
            .is_none()
    );
    assert!(matches!(
        fx.input
            .run
            .context
            .take_pair_accounting(&mut fx.input.run.clock),
        Err(RunFailure::RepairOwned)
    ));
    assert!(matches!(
        fx.input
            .run
            .context
            .advance_pairs(&mut ports(&mut fx.input.run.clock)),
        Err(RunFailure::RepairOwned)
    ));
    assert!(matches!(
        fx.input
            .run
            .context
            .bar_pending_pairs(s::NonDispatchCause::Unload, &mut fx.input.run.clock),
        Err(RunFailure::RepairOwned)
    ));
    assert!(
        fx.input
            .run
            .context
            .complete_wait(PumpDeadline::Decision)
            .is_none()
    );
    let mut cancelled = None;
    for _ in 0..512 {
        fx.step(true);
        cancelled = fx
            .input
            .run
            .context
            .take_cancelled_repair(&mut fx.input.run.clock)
            .unwrap();
        if cancelled.is_some() {
            break;
        }
    }
    assert!(
        cancelled.is_some(),
        "pair receipts must settle before cancellation returns"
    );
    let completed = fx
        .input
        .run
        .context
        .completed_pair(&call_id, &mut fx.input.run.clock)
        .unwrap()
        .unwrap();
    assert!(matches!(
        completed.ended().reason,
        Some(s::PairFailure::NotDispatched {
            cause: s::NonDispatchCause::Stop
        })
    ));
    assert!(
        fx.input
            .run
            .context
            .take_restatement(&mut fx.input.run.clock)
            .unwrap()
            .is_none()
    );
    assert!(
        fx.input
            .run
            .context
            .take_effective_emission(&mut fx.input.run.clock)
            .unwrap()
            .is_none()
    );
    assert_eq!(fx.input.records(Kind::ToolCallStarted).len(), 1);
    assert_eq!(fx.input.records(Kind::ToolCallCompleted).len(), 1);
    assert!(fx.input.run.context.failure().is_none());
}

#[test]
fn cancellation_waits_for_the_transferred_correction_and_accounts_its_send() {
    let mut fx = RepairFixture::new();
    let original = fx.generation(vec![text("bad original")], true);
    fx.begin(original);
    let request = fx.restatement(true);
    let correction = fx.generation(vec![text("Read cancelled send."), call()], false);
    let call_id = correction.requests()[0].opening().call_id.clone();
    let bound = fx
        .input
        .run
        .context
        .deadlines
        .get(PumpDeadline::Decision)
        .unwrap();
    assert!(
        fx.input
            .run
            .context
            .complete_wait(PumpDeadline::Decision)
            .is_none()
    );
    fx.input
        .run
        .context
        .cancel_repair(s::NonDispatchCause::Stop, &mut fx.input.run.clock)
        .unwrap();
    for _ in 0..4 {
        fx.step(false);
        assert!(
            fx.input
                .run
                .context
                .take_cancelled_repair(&mut fx.input.run.clock)
                .unwrap()
                .is_none()
        );
        assert_eq!(
            fx.input.run.context.deadlines.get(PumpDeadline::Decision),
            Some(bound)
        );
    }
    let progress = fx
        .input
        .run
        .context
        .observe_correction(request.source(), correction, &mut fx.input.run.clock)
        .unwrap();
    assert_eq!(progress, repair::CorrectionProgress::Cancelled);
    let mut cancelled = None;
    for _ in 0..512 {
        fx.step(true);
        cancelled = fx
            .input
            .run
            .context
            .take_cancelled_repair(&mut fx.input.run.clock)
            .unwrap();
        if cancelled.is_some() {
            break;
        }
        assert_eq!(
            fx.input.run.context.deadlines.get(PumpDeadline::Decision),
            Some(bound)
        );
    }
    assert!(
        cancelled.is_some(),
        "the returned correction must retain its pair obligation"
    );
    let completed = fx
        .input
        .run
        .context
        .completed_pair(&call_id, &mut fx.input.run.clock)
        .unwrap()
        .unwrap();
    assert!(matches!(
        completed.ended().reason,
        Some(s::PairFailure::NotDispatched {
            cause: s::NonDispatchCause::Stop
        })
    ));
    assert_eq!(fx.input.records(Kind::ToolCallStarted).len(), 1);
    assert_eq!(fx.input.records(Kind::ToolCallCompleted).len(), 1);
    assert!(
        fx.input
            .run
            .context
            .take_effective_emission(&mut fx.input.run.clock)
            .unwrap()
            .is_none()
    );
    assert!(fx.input.run.context.failure().is_none());
}

#[test]
fn cancellation_finishes_context_owned_correction_checks_and_pair_receipts() {
    for checks_complete in [false, true] {
        let mut fx = RepairFixture::new();
        let generation = fx.generation(vec![text("bad original")], true);
        fx.begin(generation);
        let _request = fx.restatement(true);
        fx.arm();
        let bound = fx
            .input
            .run
            .context
            .deadlines
            .get(PumpDeadline::Decision)
            .unwrap();
        fx.input
            .run
            .context
            .begin_output(
                original(vec![text("Read cancelled send."), call()]),
                None,
                &mut fx.input.run.clock,
            )
            .unwrap();
        let mut check = None;
        for _ in 0..512 {
            fx.input
                .run
                .context
                .advance_output(&mut ports(&mut fx.input.run.clock))
                .unwrap();
            if let Some(received) = fx.receive_check() {
                if received.text == "Read cancelled send." {
                    check = Some(received);
                    break;
                }
                fx.input.reply(&reply(&received, false));
            }
            fx.member(false);
        }
        let check = check.expect("correction must own an actual checker exchange");
        fx.input.run.context.cancel_output();
        assert!(
            !fx.input
                .run
                .context
                .flow
                .output_checks()
                .unwrap()
                .is_cancelled(),
            "generic cancellation must not steal repair's emitted correction"
        );
        if checks_complete {
            fx.input.reply(&reply(&check, false));
            let mut completed = false;
            for _ in 0..512 {
                let progress = fx
                    .input
                    .run
                    .context
                    .advance_output(&mut ports(&mut fx.input.run.clock))
                    .unwrap();
                fx.checker();
                fx.member(false);
                if progress == OutputProgress::Complete {
                    completed = true;
                    break;
                }
            }
            assert!(
                completed,
                "leave the completed checked generation with its context"
            );
        }
        fx.input
            .run
            .context
            .cancel_repair(s::NonDispatchCause::Unload, &mut fx.input.run.clock)
            .unwrap();
        assert!(
            fx.input
                .run
                .context
                .take_cancelled_repair(&mut fx.input.run.clock)
                .unwrap()
                .is_none()
        );
        assert!(
            fx.input
                .run
                .context
                .complete_wait(PumpDeadline::Decision)
                .is_none()
        );
        if !checks_complete {
            fx.input.reply(&reply(&check, false));
        }
        let mut cancelled = None;
        for _ in 0..512 {
            fx.step(true);
            cancelled = fx
                .input
                .run
                .context
                .take_cancelled_repair(&mut fx.input.run.clock)
                .unwrap();
            if cancelled.is_some() {
                break;
            }
            assert_eq!(
                fx.input.run.context.deadlines.get(PumpDeadline::Decision),
                Some(bound)
            );
        }
        assert!(
            cancelled.is_some(),
            "cancellation must finish retained selected checks and pairs"
        );
        let call_id = fx
            .filings
            .iter()
            .find_map(|frame| match frame {
                s::StateFrame::FilePair { filing, .. } => match &filing.pair {
                    s::PairRecord::Started(started) => Some(started.call_id.clone()),
                    _ => None,
                },
                _ => None,
            })
            .expect("accepted cancelled send must have its actual opening filing");
        let completed = fx
            .input
            .run
            .context
            .completed_pair(&call_id, &mut fx.input.run.clock)
            .unwrap()
            .unwrap();
        assert!(matches!(
            completed.ended().reason,
            Some(s::PairFailure::NotDispatched {
                cause: s::NonDispatchCause::Unload
            })
        ));
        assert_eq!(fx.input.records(Kind::ToolCallStarted).len(), 1);
        assert_eq!(fx.input.records(Kind::ToolCallCompleted).len(), 1);
        assert!(
            fx.input
                .run
                .context
                .take_effective_emission(&mut fx.input.run.clock)
                .unwrap()
                .is_none()
        );
        assert!(fx.input.run.context.failure().is_none());
    }
}

#[test]
fn correction_check_keeps_its_decision_bound_until_the_actual_result_records() {
    let mut fx = RepairFixture::new();
    let generation = fx.generation(vec![text("bad original")], true);
    fx.begin(generation);
    let restatement = fx.restatement(true);
    fx.arm();
    let bound = fx
        .input
        .run
        .context
        .deadlines
        .get(PumpDeadline::Decision)
        .unwrap();
    fx.input
        .run
        .context
        .begin_output(
            original(vec![text("Read blocked correction.")]),
            None,
            &mut fx.input.run.clock,
        )
        .unwrap();
    let mut request = None;
    for _ in 0..512 {
        fx.input
            .run
            .context
            .advance_output(&mut ports(&mut fx.input.run.clock))
            .unwrap();
        if let Some(received) = fx.receive_check() {
            if received.text == "Read blocked correction." {
                request = Some(received);
            } else {
                fx.input.reply(&reply(&received, false));
            }
        }
        fx.member(true);
        if request.is_some() {
            break;
        }
    }
    let request = request.expect("correction check must be in flight");
    assert!(
        fx.input
            .run
            .context
            .complete_wait(PumpDeadline::Decision)
            .is_none()
    );
    assert_eq!(
        fx.input.run.context.deadlines.get(PumpDeadline::Decision),
        Some(bound)
    );
    assert!(
        fx.input
            .run
            .context
            .take_output(&mut fx.input.run.clock)
            .unwrap()
            .is_none()
    );
    fx.input.reply(&reply(&request, false));
    let mut checked = None;
    for _ in 0..512 {
        fx.input
            .run
            .context
            .advance_output(&mut ports(&mut fx.input.run.clock))
            .unwrap();
        fx.checker();
        fx.member(true);
        checked = fx
            .input
            .run
            .context
            .take_output(&mut fx.input.run.clock)
            .unwrap();
        if checked.is_some() {
            break;
        }
    }
    let original =
        OriginalRequests::from_checked(checked.expect("recorded check must complete"), &fx.scheme)
            .unwrap();
    fx.observe(restatement.source(), original);
    assert_eq!(fx.effective(true).text(), "Read blocked correction.");
    assert!(fx.input.run.context.failure().is_none());
}
