//! The dormant placement owner through actual Open, repair, PlanSummary,
//! Released/Recorded material, decoder packets and recorder barriers.
//! Scripted SPU counts establish custody, not rendering or model qualification.

use super::*;
use crate::flow::material::SelectedUnit;
use crate::flow::place::*;
use weaver_types::{Finish, Generation, TurnKey};

const PLACE_WAIT: u64 = 1_000_000;

struct PlaceFixture {
    summary: SummaryFixture,
    decode: DecodePort,
    peer: crate::channel::ChildEnd,
    next_snapshot: u64,
    hold_files: bool,
}

impl PlaceFixture {
    fn new() -> Self {
        let mut summary = SummaryFixture::new(vec![]);
        let proof = summary.boot(None);
        let run = summary.run();
        run.context.install_place(proof, &mut run.clock).unwrap();
        let (decode, peer) = summary._decode.take().unwrap();
        Self {
            summary,
            decode,
            peer,
            next_snapshot: 100,
            hold_files: false,
        }
    }
    fn run(&mut self) -> &mut Fixture {
        self.summary.run()
    }
    fn schemas(&self) -> Vec<String> {
        self.summary
            .boot
            .member
            .run
            .context
            .loaded_place_schemas()
            .unwrap()
            .to_vec()
    }
    fn records(&self, kind: Kind) -> Vec<serde_json::Value> {
        self.summary.records(kind)
    }
    fn journal(&self) -> crate::flow::journal::BootSnapshot {
        self.summary
            .boot
            .member
            .run
            .context
            .boot_journal
            .as_ref()
            .unwrap()
            .boot_snapshot()
    }
    fn turn(&mut self, name: &str) {
        let run = self.run();
        let mut started = crate::flow::context::tests::event(Kind::TurnStarted, None);
        started.turn = Some(weaver_trace::TurnRef(name.into()));
        run.author(started).unwrap();
        assert_eq!(run.context.open_turn.as_ref().unwrap().0, name);
    }
    fn observe_place_command(&mut self, payload: weaver_types::LifecycleDirective) {
        let command = directive(payload);
        let (admin, peer) = OrganChannel::pair().unwrap();
        let mut admin = LifecyclePeer::from_channel(admin).unwrap();
        send_bytes(peer.as_fd(), &serde_json::to_vec(&command).unwrap());
        for _ in 0..8 {
            let run = &mut self.summary.boot.member.run;
            let mut io = SummaryReadyClock(&mut run.clock);
            run.context
                .pump_once(&mut PumpPorts {
                    lifecycle: LifecyclePorts {
                        listener: None,
                        peer: Some(&mut admin),
                        residency: None,
                    },
                    gate: None,
                    decode: Some(&mut self.decode),
                    label: None,
                    io: &mut io,
                })
                .unwrap();
            if admin.held().is_some() {
                break;
            }
        }
        assert_eq!(admin.take_completion().unwrap().unwrap().envelope, command);
        assert!(
            receive(peer.as_fd()).is_none(),
            "fixture consumes command, not an invented reply"
        );
    }
    fn close_turn(&mut self) {
        let run = self.run();
        let turn = run.context.open_turn.clone().unwrap();
        run.author(AuthoredEvent {
            kind: Kind::TurnClosed,
            subsystem: weaver_trace::Subsystem::Harness,
            turn: Some(turn),
            source: AuthoredSource::Runtime,
            causal_parent: None,
            payload: Some(Payload::TurnClosed(weaver_trace::TurnClose::Clean)),
        })
        .unwrap();
        assert!(run.context.open_turn.is_none());
    }
    fn decision(&mut self) -> AbsoluteBound {
        let run = self.run();
        if let Some(bound) = run.context.deadlines.get(PumpDeadline::Decision) {
            return bound;
        }
        let bound = AbsoluteBound::after(run.clock.now().unwrap(), PLACE_WAIT).unwrap();
        run.context
            .arm(PumpDeadline::Decision, bound, &mut run.clock)
            .unwrap();
        bound
    }
    fn read(&mut self, selector: s::ReadSelector, result: s::QueryResult) -> RecordedRead {
        let snapshot = self.next_snapshot;
        self.next_snapshot += 1;
        let raw = serde_json::to_vec_pretty(&result).unwrap();
        let member = &mut self.summary.boot.member;
        member
            .run
            .context
            .begin_read(selector, None, &mut member.run.clock)
            .unwrap();
        let request = member.next_request();
        member.respond(&request, page(snapshot, 0, raw.len(), &raw));
        let release = member.next_request();
        assert!(matches!(&release, s::StateFrame::Control {
            frame: s::StateControl::Release { snapshot_id }, .. } if snapshot_id.0 == snapshot));
        member.respond(
            &release,
            s::StateControlAnswer::Released {
                snapshot_id: snapshot.into(),
            },
        );
        let proof = member.finish().unwrap();
        assert_eq!(proof.raw_bytes(), raw);
        proof
    }
    fn material(
        &mut self,
        selector: s::ReadSelector,
        entries: Vec<s::ReadEntry>,
    ) -> Vec<SelectedUnit> {
        let proof = self.read(selector, s::QueryResult::Entries { entries });
        let run = self.run();
        run.context
            .hold_material(proof, &mut run.clock)
            .unwrap()
            .into_units()
    }
    fn text(&mut self, sequence: u64, noun: &str) -> Vec<SelectedUnit> {
        self.material(
            s::ReadSelector::History {
                noun: noun.into(),
                verb: None,
                condition: None,
            },
            vec![text_entry(sequence, noun)],
        )
    }
    fn whole_span(&mut self, sequence: u64, noun: &str, span_id: u64) -> Vec<SelectedUnit> {
        let span = s::SpanRef {
            seed_id: self.summary.seed().into(),
            local_id: span_id.into(),
            revision: 1.into(),
        };
        let mut entry = text_entry(sequence, noun);
        entry.selected_span = Some(s::SelectedSpan {
            span_ref: span.clone(),
            start: 0.into(),
            end: (entry.text.len() as u64).into(),
        });
        self.material(
            s::ReadSelector::Span {
                seed_id: span.seed_id,
                local_id: span.local_id,
                revision: span.revision,
            },
            vec![entry],
        )
    }
    fn request(&mut self, selection: Vec<SelectedUnit>) -> PlaceRequest {
        self.decision();
        let schemas = self.schemas();
        let run = self.run();
        let prefix = run
            .context
            .prepare_prefix(None, s::CutMethod::Flush, &mut run.clock)
            .unwrap();
        PlaceRequest {
            prefix,
            selection,
            tool_schemas: schemas,
            context: s::PlacementContext {
                label: "actual held fixture".into(),
                instruction: None,
            },
        }
    }
    fn begin(&mut self, request: PlaceRequest) {
        let run = &mut self.summary.boot.member.run;
        run.context
            .begin_place(request, &mut self.decode, &mut run.clock)
            .unwrap();
    }
    fn advance(&mut self) -> Result<PlaceProgress, RunFailure> {
        let run = &mut self.summary.boot.member.run;
        let mut io = ReadyClock {
            clock: &mut run.clock,
            fd: self.decode.fd().as_raw_fd(),
        };
        run.context.advance_place(&mut PumpPorts {
            lifecycle: LifecyclePorts::default(),
            gate: None,
            decode: Some(&mut self.decode),
            label: None,
            io: &mut io,
        })
    }
    fn next_measure(&mut self) -> TokenDirective {
        for _ in 0..128 {
            if let Some(raw) = receive(self.peer.as_fd()) {
                let request: TokenDirective = serde_json::from_slice(&raw).unwrap();
                assert!(
                    matches!(request, TokenDirective::MeasureDelta { .. }),
                    "prepare must only measure: {request:?}"
                );
                return request;
            }
            self.advance().unwrap();
        }
        panic!("placement did not send its exact measurement");
    }
    fn answer(&self, answer: &TokenAnswer) -> Vec<u8> {
        let raw = serde_json::to_vec(answer).unwrap();
        send_bytes(self.peer.as_fd(), &raw);
        raw
    }
    fn measured(&self, keep: u64, input: u64) -> TokenAnswer {
        let snapshot = self.journal();
        TokenAnswer::DeltaMeasured {
            resident: snapshot.1,
            prefix_tokens: snapshot.0,
            keep,
            input_tokens: input,
            capacity: snapshot.2,
        }
    }
    fn take(&mut self) -> PlacementCompletion {
        for _ in 0..128 {
            self.advance().unwrap();
            let run = self.run();
            if let Some(done) = run.context.take_placement(&mut run.clock).unwrap() {
                return done;
            }
        }
        panic!("placement did not finish");
    }
    fn grant(&mut self, selection: Vec<SelectedUnit>, input: u64) -> PlacementGrant {
        let request = self.request(selection);
        self.begin(request);
        let TokenDirective::MeasureDelta { keep, .. } = self.next_measure() else {
            unreachable!()
        };
        self.answer(&self.measured(keep, input));
        let PlacementCompletion::Granted(grant) = self.take() else {
            panic!("valid placement refused");
        };
        assert!(
            receive(self.peer.as_fd()).is_none(),
            "preparation may not send AppendAndGenerate"
        );
        grant
    }
    fn start_generation(&mut self, grant: PlacementGrant) {
        let run = &mut self.summary.boot.member.run;
        run.context
            .begin_generation(grant, &mut self.decode, &mut run.clock)
            .unwrap();
    }
    fn advance_generation(&mut self) -> Result<GenerationProgress, RunFailure> {
        let run = &mut self.summary.boot.member.run;
        let mut io = ReadyClock {
            clock: &mut run.clock,
            fd: self.decode.fd().as_raw_fd(),
        };
        let result = run.context.advance_generation(&mut PumpPorts {
            lifecycle: LifecyclePorts::default(),
            gate: None,
            decode: Some(&mut self.decode),
            label: None,
            io: &mut io,
        });
        if result.is_ok() {
            self.classifier(false);
            if !self.hold_files {
                self.summary.file_reply();
            }
        }
        result
    }
    fn classifier(&mut self, reject: bool) {
        self.summary.transport(TransportSource::Classifier);
        let Receive::Complete(message) = self.summary.boot._classifier.try_receive().unwrap()
        else {
            return;
        };
        assert_eq!(message.tag, Tag::Request);
        let request: s::Ste100Request =
            s::decode_strict(std::str::from_utf8(&message.payload).unwrap()).unwrap();
        let text = request.text.as_str();
        let noun = text
            .strip_prefix("Read ")
            .and_then(|s| s.strip_suffix('.'))
            .expect("fixture emits independently span-checked Read statements");
        let len = text.len() as u64;
        let result = if reject {
            s::Ste100Result::NotSte100 {
                code: s::Ste100DefectCode::NonSte100,
                detail: "fixture rejection".into(),
                spans: vec![],
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
                        value: noun.into(),
                        span: s::Span {
                            start: 5.into(),
                            end: (len - 1).into(),
                        },
                    }],
                }],
            }
        };
        let reply = s::Ste100Reply {
            request_id: request.request_id,
            target: request.target,
            scheme_id: request.scheme_id,
            result,
        };
        self.summary
            .boot
            ._classifier
            .begin_send(
                Tag::Reply,
                reply.request_id.0,
                serde_json::to_vec(&reply).unwrap(),
            )
            .unwrap();
        while self.summary.boot._classifier.has_pending_send() {
            self.summary.boot._classifier.try_send().unwrap();
        }
        self.summary.transport(TransportSource::Classifier);
    }
    fn finish_generation(&mut self) -> GenerationCompletion {
        for _ in 0..128 {
            self.advance_generation().unwrap();
            let run = self.run();
            if let Some(done) = run.context.take_placed_generation(&mut run.clock).unwrap() {
                if !self.hold_files {
                    self.drain_files();
                }
                return done;
            }
        }
        panic!("generation did not settle its real terminal");
    }
    fn drain_files(&mut self) {
        for _ in 0..128 {
            let run = self.run();
            run.context.reduce_flow(&mut run.clock).unwrap();
            self.classifier(false);
            self.summary.file_reply();
            let run = self.run();
            if run.context.flow.ledger().facts().next().is_none()
                && !run.context.flow.has_pending_record()
                && run.context.flow.state().outstanding().is_empty()
            {
                return;
            }
        }
        panic!("actual assistant filing did not settle");
    }
    fn generation_failure(&mut self) -> RunFailure {
        for _ in 0..128 {
            if let Err(failure) = self.advance_generation() {
                return failure;
            }
        }
        panic!("invalid generation granted or remained live");
    }
    fn generation(
        &mut self,
        units: Vec<SelectedUnit>,
        input: usize,
        output: usize,
        text: &str,
        finish: Finish,
    ) -> ConfirmedGeneration {
        let keep = self.journal().1;
        let capacity = self.journal().2;
        let grant = self.grant(units, input as u64);
        self.start_generation(grant);
        self.sent_generation();
        let answer = generated(keep, input, output, text, finish, capacity);
        let raw = self.answer(&TokenAnswer::Generated(answer.clone()));
        let GenerationCompletion::Generated(proof) = self.finish_generation() else {
            panic!("matching complete generation refused");
        };
        assert_eq!(proof.generation(), &answer);
        assert_eq!(proof.raw_bytes(), raw);
        *proof
    }
    fn sent_generation(&mut self) -> TokenDirective {
        for _ in 0..128 {
            if let Some(raw) = receive(self.peer.as_fd()) {
                let request: TokenDirective = serde_json::from_slice(&raw).unwrap();
                assert!(matches!(request, TokenDirective::AppendAndGenerate { .. }));
                return request;
            }
            self.advance_generation().unwrap();
        }
        panic!("generation did not send its consumed grant");
    }
}

fn text_entry(sequence: u64, noun: &str) -> s::ReadEntry {
    let text = format!("Read {noun}.");
    let len = text.len() as u64;
    let block = s::SourceBlockId {
        run: "held-old-run".into(),
        sequence: sequence.into(),
        block: 0.into(),
    };
    s::ReadEntry {
        source: s::ReadSource::Event(Box::new(s::EventSource {
            block_id: block,
            envelope: s::SourceEnvelope {
                session: "held-old-session".into(),
                run: "held-old-run".into(),
                turn: Some("held-old-turn".into()),
                sequence: sequence.into(),
                kind: "message.assistant".into(),
                channel: "output".into(),
                origin: "model".into(),
                subsystem: "harness".into(),
                causal_parent: None,
                wall_ms: 0,
                monotonic_ns: 10.into(),
            },
            source_path: vec![
                s::SourcePathPart::Key("content".into()),
                s::SourcePathPart::Index { index: 0.into() },
            ],
            partition_rule_id: "assistant".into(),
        })),
        raw_block: serde_json::value::to_raw_value(&serde_json::json!({"type":"text","text":text}))
            .unwrap(),
        text,
        bindings: vec![s::ReadBinding {
            ordinal: 0.into(),
            value: s::Binding {
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
                    value: noun.into(),
                    span: s::Span {
                        start: 5.into(),
                        end: (len - 1).into(),
                    },
                }],
            },
        }],
        selected_span: None,
        step_query: None,
        pairs: vec![],
    }
}

fn generated(
    keep: u64,
    input: usize,
    output: usize,
    text: &str,
    finish: Finish,
    capacity: u64,
) -> Generation {
    Generation {
        emission: text.into(), finish,
        content: if text.is_empty() { vec![] } else { vec![ContentBlock::Text { text: text.into() }] },
        unrecovered: vec![],
        request: serde_json::value::RawValue::from_string("{ \"rendered\" : \"fixture family render\", \"template\" : \"fixture\" }".into()).unwrap(),
        measurement: serde_json::value::to_raw_value(&serde_json::json!({
            "model":"fixture", "weights_hash":"fixture", "input_tokens":vec![17_u32; input], "output_tokens":vec![31_u32; output]
        })).unwrap(),
        resident: keep + input as u64 + output as u64 + 1, capacity,
    }
}

#[test]
fn place_prepare_uses_actual_boot_summary_and_material_without_sending_append() {
    let mut fx = PlaceFixture::new();
    let units = fx.text(8, "record");
    let messages = units
        .iter()
        .flat_map(|unit| unit.messages().iter().cloned())
        .collect::<Vec<_>>();
    fx.turn("placement-turn");
    let before = fx.journal();
    let request = fx.request(units);
    let assembly = request.prefix.assembly_id();
    assert_eq!(request.prefix.n(), before.1);
    fx.begin(request);
    let TokenDirective::MeasureDelta {
        keep,
        delta,
        tool_schemas,
    } = fx.next_measure()
    else {
        unreachable!()
    };
    assert_eq!(delta, messages);
    assert_eq!(tool_schemas, fx.schemas());
    assert_eq!(keep, before.1);
    assert_eq!(fx.journal().6, 0, "measurement is no resident occurrence");
    fx.answer(&fx.measured(keep, 3));
    let PlacementCompletion::Granted(grant) = fx.take() else {
        panic!("placement refused");
    };
    assert_eq!(grant.assembly_id(), assembly);
    assert_eq!(grant.delta(), messages);
    assert_eq!(grant.tool_schemas(), fx.schemas());
    assert_eq!(
        fx.journal().6,
        0,
        "Recorded proposal is not generation confirmation"
    );
    assert_eq!(fx.records(Kind::StatePlacement).len(), 2);
    let record = fx.records(Kind::StatePlacement).pop().unwrap();
    assert_eq!(record["sequence"], grant.placement_sequence().0.to_string());
    assert_eq!(record["payload"]["phase"], "append");
    assert!(record["payload"]["open_evidence"].is_null());
    assert!(receive(fx.peer.as_fd()).is_none());
    fx.start_generation(grant);
    let TokenDirective::AppendAndGenerate {
        turn,
        delta,
        tool_schemas,
    } = fx.sent_generation()
    else {
        unreachable!()
    };
    assert_eq!(turn, TurnKey("placement-turn".into()));
    assert_eq!(delta, messages);
    assert_eq!(tool_schemas, fx.schemas());
    assert_eq!(
        fx.journal().6,
        0,
        "complete request send is not generation confirmation"
    );
}

const _: fn() = || {
    trait Private<M> {
        fn pin() {}
    }
    impl<T: ?Sized> Private<()> for T {}
    impl<T: Clone> Private<u8> for T {}
    impl<T: Default> Private<u16> for T {}
    impl<T: From<Generation>> Private<u32> for T {}
    impl<T: From<s::ArgumentCursor>> Private<u64> for T {}
    impl<T: From<u64>> Private<i8> for T {}
    let _ = <PreparedPrefix as Private<_>>::pin;
    let _ = <PlacementGrant as Private<_>>::pin;
    let _ = <ConfirmedGeneration as Private<_>>::pin;
    let _ = <Staged as Private<_>>::pin;
    let _ = <PendingReloadText as Private<_>>::pin;
};

#[test]
fn place_complete_terminals_install_real_messages_and_empty_assistant_once() {
    for (finish, text, output) in [
        (Finish::Completed, "Read result.", 2),
        (Finish::Length, "Read result.", 1),
        (Finish::Stopped, "Read result.", 1),
        (Finish::Completed, "", 0),
    ] {
        let mut fx = PlaceFixture::new();
        let units = fx.text(8, "caf\u{e9}");
        fx.turn("confirmed-turn");
        let before = fx.journal();
        let proof = fx.generation(units, 4, output, text, finish);
        assert_eq!(proof.turn().0, "confirmed-turn");
        assert_eq!(fx.journal().1, before.1 + 4 + output as u64 + 1);
        assert_eq!(
            fx.journal().6,
            2,
            "delta plus even an empty assistant occurrence"
        );
        assert_eq!(fx.records(Kind::StatePlacement).len(), 2);
        let run = fx.run();
        assert!(
            run.context
                .take_placed_generation(&mut run.clock)
                .unwrap()
                .is_none()
        );
        assert!(run.context.pending_author().is_none());
        assert!(run.context.completed_author.is_none());
        // The next legitimate measurement/generation must reuse confirmed N,
        // not the old prefix, and must not inherit a consumed completion slot.
        let next = fx.text(9, "record");
        let keep = fx.journal().1;
        let next_proof = fx.generation(next, 2, 1, "Read next.", Finish::Completed);
        assert!(next_proof.assistant_id().0 > proof.assistant_id().0);
        assert_eq!(fx.journal().1, keep + 2 + 1 + 1);
        assert_eq!(fx.journal().6, 4);
    }
}

#[test]
fn place_measurement_wrong_journal_facts_never_grant_or_install_an_occurrence() {
    for bad in ["prefix", "resident", "keep", "capacity"] {
        let mut fx = PlaceFixture::new();
        let units = fx.text(8, "record");
        fx.turn("measure-facts");
        let request = fx.request(units);
        fx.begin(request);
        let TokenDirective::MeasureDelta { keep, .. } = fx.next_measure() else {
            unreachable!()
        };
        let mut answer = fx.measured(keep, 3);
        let TokenAnswer::DeltaMeasured {
            prefix_tokens,
            resident,
            keep,
            capacity,
            ..
        } = &mut answer
        else {
            unreachable!()
        };
        match bad {
            "prefix" => *prefix_tokens += 1,
            "resident" => *resident += 1,
            "keep" => *keep += 1,
            "capacity" => *capacity -= 1,
            _ => unreachable!(),
        }
        fx.answer(&answer);
        let mut failure = None;
        for _ in 0..128 {
            if let Err(found) = fx.advance() {
                failure = Some(found);
                break;
            }
        }
        assert!(failure.is_some(), "mismatched {bad} must fail");
        assert!(fx.run().context.failure().is_some());
        assert_eq!(fx.journal().6, 0);
        assert_eq!(
            fx.records(Kind::StatePlacement).len(),
            1,
            "bad measurement has no append placement"
        );
        assert!(receive(fx.peer.as_fd()).is_none());
    }
}

#[test]
fn place_generated_token_counts_are_independent_checked_facts() {
    for bad in [
        "missing-input",
        "missing-output",
        "input-count",
        "output-limit",
        "resident",
        "capacity",
        "negative-token",
        "floating-token",
        "wide-token",
        "string-token",
        "object-array",
        "duplicate-key",
    ] {
        let mut fx = PlaceFixture::new();
        let units = fx.text(8, "record");
        fx.turn("bad-terminal");
        let before = fx.journal();
        let grant = fx.grant(units, 3);
        fx.start_generation(grant);
        fx.sent_generation();
        let mut answer = generated(before.1, 3, 2, "Read result.", Finish::Completed, before.2);
        let mut measurement: serde_json::Value =
            serde_json::from_str(answer.measurement.get()).unwrap();
        match bad {
            "missing-input" => {
                measurement.as_object_mut().unwrap().remove("input_tokens");
            }
            "missing-output" => {
                measurement.as_object_mut().unwrap().remove("output_tokens");
            }
            "input-count" => {
                measurement["input_tokens"] = serde_json::json!([17, 17]);
                answer.resident -= 1;
            }
            "output-limit" => {
                let limit = fx.run().context.place_engine.as_ref().unwrap().limits().2;
                assert_eq!(limit, 16, "actual admitted fixture limit");
                measurement["output_tokens"] = serde_json::json!(vec![31_u32; limit as usize + 1]);
                answer.resident = before.1 + 3 + limit + 1 + 1;
                assert!(
                    answer.resident < answer.capacity,
                    "only the generation limit is exceeded"
                );
            }
            "resident" => answer.resident += 1,
            "capacity" => answer.capacity -= 1,
            "negative-token" => measurement["output_tokens"][0] = serde_json::json!(-1),
            "floating-token" => measurement["input_tokens"][0] = serde_json::json!(1.5),
            "wide-token" => {
                measurement["output_tokens"][0] = serde_json::json!(u64::from(u32::MAX) + 1)
            }
            "string-token" => measurement["input_tokens"][0] = serde_json::json!("17"),
            "object-array" => measurement["output_tokens"] = serde_json::json!({"0":31,"1":31}),
            "duplicate-key" => {}
            _ => unreachable!(),
        }
        answer.measurement = if bad == "duplicate-key" {
            serde_json::value::RawValue::from_string(
                "{\"input_tokens\":[17,17,17],\"output_tokens\":[31,31],\"output_tokens\":[31,31]}"
                    .into(),
            )
            .unwrap()
        } else {
            serde_json::value::to_raw_value(&measurement).unwrap()
        };
        let raw = fx.answer(&TokenAnswer::Generated(answer));
        let failure = fx.generation_failure();
        assert_eq!(fx.run().context.failure(), Some(&failure), "{bad}");
        assert_eq!(
            fx.journal().1,
            before.1,
            "{bad}: invalid terminal cannot change resident"
        );
        assert_eq!(
            fx.journal().6,
            0,
            "{bad}: invalid terminal cannot install messages"
        );
        assert!(!raw.is_empty());
        assert!(receive(fx.peer.as_fd()).is_none());
    }
}

#[test]
fn place_wrong_terminal_arm_and_nonassistant_content_never_confirm_residency() {
    for bad in ["at-rest", "refed", "flushed", "tool-result"] {
        let mut fx = PlaceFixture::new();
        let units = fx.text(8, "record");
        fx.turn("wrong-terminal");
        let before = fx.journal();
        let grant = fx.grant(units, 3);
        fx.start_generation(grant);
        fx.sent_generation();
        let mut generation = generated(before.1, 3, 2, "Read result.", Finish::Completed, before.2);
        let answer = match bad {
            "at-rest" => TokenAnswer::AtRest,
            "refed" => TokenAnswer::ReFed(generation),
            "flushed" => TokenAnswer::Flushed {
                resident_before: before.1,
                resident_after: before.0,
            },
            "tool-result" => {
                generation.content =
                    vec![ContentBlock::ToolResult(weaver_traits::ToolResultBlock {
                        content: "not an assistant block".into(),
                    })];
                TokenAnswer::Generated(generation)
            }
            _ => unreachable!(),
        };
        fx.answer(&answer);
        fx.generation_failure();
        assert_eq!(fx.journal().1, before.1, "{bad}");
        assert_eq!(fx.journal().6, 0, "{bad}");
    }
}

#[test]
fn place_schema_mismatch_returns_exact_request_before_any_measure_or_record() {
    let mut fx = PlaceFixture::new();
    let units = fx.text(8, "record");
    fx.turn("wrong-schemas");
    let mut request = fx.request(units);
    request.tool_schemas.push("caller override".into());
    let expected = request.tool_schemas.clone();
    let original = request.selection[0].reads()[0].recall_sequence();
    let run = &mut fx.summary.boot.member.run;
    let error = run
        .context
        .begin_place(request, &mut fx.decode, &mut run.clock)
        .unwrap_err();
    assert_eq!(error.input.tool_schemas, expected);
    assert_eq!(
        error.input.selection[0].reads()[0].recall_sequence(),
        original
    );
    assert!(run.context.failure().is_none());
    assert!(receive(fx.peer.as_fd()).is_none());
    assert_eq!(fx.records(Kind::StatePlacement).len(), 1);
}

#[test]
fn place_cancel_before_first_generation_byte_preserves_resident_and_never_sends_append() {
    let mut fx = PlaceFixture::new();
    let units = fx.text(8, "record");
    fx.turn("cancel-unsent");
    let before = fx.journal();
    let grant = fx.grant(units, 3);
    let consumed_assistant = grant.assistant_id();
    fx.start_generation(grant);
    let run = &mut fx.summary.boot.member.run;
    run.context
        .cancel_generation(&mut fx.decode, &mut run.clock)
        .unwrap();
    assert!(matches!(
        fx.finish_generation(),
        GenerationCompletion::Cancelled(_)
    ));
    assert!(receive(fx.peer.as_fd()).is_none());
    assert_eq!(fx.journal().1, before.1);
    assert_eq!(fx.journal().6, 0);
    let units = fx.text(9, "retry");
    let next = fx.grant(units, 2);
    assert!(
        next.assistant_id().0 > consumed_assistant.0,
        "cancelled IDs must never be reused"
    );
}

#[test]
fn place_inflight_cancel_retains_generated_and_drains_its_separate_at_rest_leg() {
    for natural_race in [false, true] {
        let mut fx = PlaceFixture::new();
        let units = fx.text(8, "record");
        fx.turn("original-cancel-turn");
        let before = fx.journal();
        let grant = fx.grant(units, 3);
        fx.start_generation(grant);
        fx.sent_generation();
        if !natural_race {
            fx.answer(&TokenAnswer::Token {
                token: 31,
                piece: "Read ".into(),
            });
            for _ in 0..4 {
                fx.advance_generation().unwrap();
            }
        }
        let terminal = generated(
            before.1,
            3,
            2,
            "Read result.",
            if natural_race {
                Finish::Completed
            } else {
                Finish::Stopped
            },
            before.2,
        );
        // The natural close may already be on the socket when the caller asks
        // to cancel. That does not erase the separate cancellation exchange.
        let queued_raw = natural_race.then(|| fx.answer(&TokenAnswer::Generated(terminal.clone())));
        let run = &mut fx.summary.boot.member.run;
        run.context
            .cancel_generation(&mut fx.decode, &mut run.clock)
            .unwrap();
        let mut cancellation = None;
        for _ in 0..32 {
            if let Some(raw) = receive(fx.peer.as_fd()) {
                cancellation = Some(serde_json::from_slice::<TokenDirective>(&raw).unwrap());
                break;
            }
            fx.advance_generation().unwrap();
        }
        assert_eq!(
            cancellation,
            Some(TokenDirective::Cancel {
                turn: TurnKey("original-cancel-turn".into())
            })
        );
        let raw =
            queued_raw.unwrap_or_else(|| fx.answer(&TokenAnswer::Generated(terminal.clone())));
        for _ in 0..8 {
            fx.advance_generation().unwrap();
            let run = fx.run();
            assert!(
                run.context
                    .take_placed_generation(&mut run.clock)
                    .unwrap()
                    .is_none(),
                "Generated cannot return the decode port while Cancel still owes AtRest"
            );
        }
        assert!(
            receive(fx.peer.as_fd()).is_none(),
            "no retry or next request before AtRest"
        );
        fx.answer(&TokenAnswer::AtRest);
        let GenerationCompletion::Cancelled(account) = fx.finish_generation() else {
            panic!("cancelled terminal must remain an account");
        };
        let proof = account
            .confirmed_generation()
            .expect("actual complete partial terminal remains residency evidence");
        assert_eq!(proof.generation(), &terminal);
        assert_eq!(proof.raw_bytes(), raw);
        assert_eq!(fx.journal().1, terminal.resident);
        assert_eq!(fx.journal().6, 2);
        // The next actual exchange demonstrates that AtRest was consumed by
        // its own cancellation, not silently discarded by another owner.
        let units = fx.text(9, "next");
        let proof = fx.generation(units, 2, 1, "Read next.", Finish::Completed);
        assert_eq!(proof.generation().resident, terminal.resident + 4);
    }
}

#[test]
fn place_valid_measurement_tail_refusal_records_close_without_component_latch_or_append() {
    let mut fx = PlaceFixture::new();
    let units = fx.text(8, "record");
    fx.turn("refused-fit");
    let before = fx.journal();
    let request = fx.request(units);
    fx.begin(request);
    let TokenDirective::MeasureDelta { keep, .. } = fx.next_measure() else {
        unreachable!()
    };
    fx.answer(&fx.measured(keep, u64::MAX));
    let PlacementCompletion::Refused(refused) = fx.take() else {
        panic!("oversized tail granted");
    };
    assert_eq!(refused.code, s::PlacementCode::TailOverCapacity);
    assert!(fx.run().context.failure().is_none());
    assert_eq!(fx.journal().1, before.1);
    assert_eq!(fx.journal().6, 0);
    assert!(receive(fx.peer.as_fd()).is_none());
    assert_eq!(fx.records(Kind::Refusal).len(), 1);
    assert_eq!(fx.records(Kind::TurnClosed).len(), 1);
    assert!(fx.run().context.open_turn.is_none());
    let run = fx.run();
    let response: serde_json::Value =
        serde_json::from_slice(&run.context.place_close().unwrap().octets().unwrap()).unwrap();
    assert_eq!(response["kind"], "stopped");
    assert_eq!(response["turn"], "refused-fit");
    assert_eq!(
        run.context
            .prepare_prefix(None, s::CutMethod::Flush, &mut run.clock)
            .unwrap_err(),
        PlaceError::Busy
    );
    run.context.place_response_sent(&mut run.clock).unwrap();
    assert!(
        run.context
            .prepare_prefix(None, s::CutMethod::Flush, &mut run.clock)
            .is_ok()
    );
}

#[path = "context_place_account_tests.rs"]
mod account_tests;
#[path = "context_place_adapter_tests.rs"]
mod adapter_tests;
#[path = "context_place_boundary_tests.rs"]
mod boundary_tests;
#[path = "context_place_cut_tests.rs"]
mod cut_tests;
#[path = "context_place_material_tests.rs"]
mod material_tests;
#[path = "context_place_native_tests.rs"]
mod native_tests;

#[path = "context_place_append_review_tests.rs"]
mod append_review_tests;
