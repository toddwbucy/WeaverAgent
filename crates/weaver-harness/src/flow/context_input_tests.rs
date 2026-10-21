//! Input transactions through actual recorder, state and STE100 socket owners.
//! The scheme and replies are synthetic mechanics fixtures, not qualification.

use super::super::clock::PumpIo;
use super::super::coordinator::{AdmissionAdvance, TransportSource};
use super::super::coordinator::{CheckOperation, OperationStart};
use super::super::input::{AcceptedInput, InputState};
use super::super::ledger::LedgerFile;
use super::super::pump::LifecyclePorts;
use super::super::ste100_channel::{Channel, Receive, Tag};
use super::super::ste100_socket::AdmissionPoll;
use super::tests::{Fixture, config};
use super::*;

#[path = "context_classifier_release_tests.rs"]
mod classifier_release;
#[path = "context_observation_tests.rs"]
mod observation_ticks;
use nix::sys::socket::{AddressFamily, SockFlag, SockType, socketpair};
use std::io::Read;
use std::time::{Duration, Instant};
use weaver_trace::{Subsystem, TurnRef};
use weaver_traits::{ContentBlock, Role};
use weaver_types::state_flow as s;
use weaver_types::{ArtifactRef, DeviceOrdinal, ModelBinding, TurnFrame};

fn ports<I: PumpIo>(io: &mut I) -> PumpPorts<'_, I> {
    PumpPorts {
        lifecycle: LifecyclePorts::default(),
        gate: None,
        decode: None,
        label: None,
        io,
    }
}

pub(super) struct InputFixture {
    pub(super) run: Fixture,
    pub(super) peer: Channel,
}

fn duty() -> (s::Ste100Bootstrap, s::SchemeDeclaration, s::Ste100Ready) {
    let seed: s::SeedRequest = s::decode_strict(include_str!("fixtures/seed.json")).unwrap();
    let scheme = s::SchemeDeclaration {
        artifact_sha256: s::Digest::new(seed.scheme_id.strip_prefix("ste100:").unwrap()).unwrap(),
        scheme_id: seed.scheme_id,
        verbs: vec!["read".into()],
    };
    let instruction = s::Ste100Instruction {
        model_binding: ModelBinding {
            artifact: ArtifactRef("synthetic-input-transaction-fixture".into()),
            devices: vec![DeviceOrdinal(0)],
        },
        model_artifact_sha256: seed.checker_artifact_sha256,
        instances: 1,
        max_wire_bytes: 262144.into(),
        measurement_ref: "synthetic transport, not qualification".into(),
        repair_cap: s::RepairCap {
            attempts: 2,
            measurement_ref: "fixture".into(),
        },
        decision_wait: s::MeasuredWait {
            elapsed_ns: 10000.into(),
            measurement_ref: "fixture".into(),
        },
    };
    let ready = s::Ste100Ready {
        protocol: s::STE100_PROTOCOL.into(),
        scheme_id: scheme.scheme_id.clone(),
        artifact_sha256: scheme.artifact_sha256.clone(),
        checker_artifact_sha256: instruction.model_artifact_sha256.clone(),
        instances: instruction.instances,
        max_wire_bytes: instruction.max_wire_bytes,
    };
    (
        s::Ste100Bootstrap {
            protocol: s::STE100_PROTOCOL.into(),
            scheme_text: seed.ste100_scheme,
            instruction,
        },
        scheme,
        ready,
    )
}

fn flush_peer(peer: &mut Channel) {
    for _ in 0..64 {
        peer.try_send().unwrap();
        if !peer.has_pending_send() {
            return;
        }
    }
    panic!("fixture peer did not finish its bounded message");
}

impl InputFixture {
    pub(super) fn new() -> Self {
        Self::with_templates(None)
    }

    pub(super) fn with_template_inventory(
        inventory: &super::super::templates::CompiledInventory,
    ) -> Self {
        Self::with_templates(Some(inventory))
    }

    pub(super) fn with_template_inventory_and_scheme(
        inventory: &super::super::templates::CompiledInventory,
        scheme_text: String,
        scheme: s::SchemeDeclaration,
    ) -> Self {
        Self::with_declaration_scheme(Some(inventory), config(), Some((scheme_text, scheme)))
    }

    pub(super) fn with_pending_limit(blocks: u64) -> Self {
        let mut declaration = config();
        declaration.backlog.max_pending_blocks = blocks.into();
        Self::with_declaration(None, declaration)
    }

    fn with_templates(inventory: Option<&super::super::templates::CompiledInventory>) -> Self {
        Self::with_declaration(inventory, config())
    }

    pub(super) fn with_declaration(
        inventory: Option<&super::super::templates::CompiledInventory>,
        declaration: FlowConfig,
    ) -> Self {
        Self::with_declaration_scheme(inventory, declaration, None)
    }

    pub(super) fn with_scheme(
        declaration: FlowConfig,
        scheme_text: String,
        scheme: s::SchemeDeclaration,
    ) -> Self {
        Self::with_declaration_scheme(None, declaration, Some((scheme_text, scheme)))
    }

    fn with_declaration_scheme(
        inventory: Option<&super::super::templates::CompiledInventory>,
        mut declaration: FlowConfig,
        override_scheme: Option<(String, s::SchemeDeclaration)>,
    ) -> Self {
        let (mut bootstrap, mut scheme, mut ready) = duty();
        if let Some((text, declared)) = override_scheme {
            bootstrap.scheme_text = text;
            scheme = declared;
            ready.scheme_id = scheme.scheme_id.clone();
            ready.artifact_sha256 = scheme.artifact_sha256.clone();
        }
        declaration.scheme = scheme;
        let mut run = Fixture::new(declaration);
        run.opened();
        let mut authored = run.load_event();
        let Some(Payload::Elections(load)) = &mut authored.payload else {
            unreachable!();
        };
        load.ste100_scheme = bootstrap.scheme_text.clone();
        load.state_flow.ste100 = super::super::mirror::instruction(&bootstrap.instruction);
        load.template_inventory.checker_artifact_sha256 =
            bootstrap.instruction.model_artifact_sha256.as_str().into();
        if let Some(inventory) = inventory {
            load.template_inventory = inventory.record(
                &run.context.declaration.scheme,
                &bootstrap.instruction.model_artifact_sha256,
            );
        }
        assert_eq!(run.author(authored).unwrap(), Sequence(0));
        run.context.activate(&mut run.clock).unwrap();
        for _ in 0..16 {
            run.context.reduce_flow(&mut run.clock).unwrap();
            if run.context.flow.operation_completion().is_some() {
                break;
            }
        }
        assert!(matches!(
            run.context
                .take_operation_completion(&mut run.clock)
                .unwrap(),
            Some(OperationCompletion::Opened { .. })
        ));

        let (end, peer) = socketpair(
            AddressFamily::Unix,
            SockType::SeqPacket,
            None,
            SockFlag::SOCK_CLOEXEC,
        )
        .unwrap();
        let mut peer = Channel::from_owned(peer, ready.max_wire_bytes.0 as usize).unwrap();
        run.context
            .begin_classifier(end, bootstrap.clone(), &mut run.clock)
            .unwrap();
        let mut received = false;
        for _ in 0..64 {
            assert_eq!(
                run.context
                    .flow
                    .advance_classifier_admission(&mut FlowRecording {
                        barrier: &mut run.context.barrier,
                        inbox: &mut run.context.inbox,
                        budget: &mut run.context.budget,
                        author: &run.context.author,
                    })
                    .unwrap(),
                AdmissionAdvance::Pending
            );
            if let Receive::Complete(frame) = peer.try_receive().unwrap() {
                assert_eq!((frame.tag, frame.id), (Tag::Bootstrap, 0));
                let actual: s::Ste100Bootstrap =
                    s::decode_strict(std::str::from_utf8(&frame.payload).unwrap()).unwrap();
                assert_eq!(actual, bootstrap);
                received = true;
                break;
            }
        }
        assert!(received, "the actual bootstrap did not cross the socket");
        peer.begin_send(Tag::Ready, 0, serde_json::to_vec(&ready).unwrap())
            .unwrap();
        flush_peer(&mut peer);
        assert_eq!(
            run.context
                .flow
                .advance_classifier_admission(&mut FlowRecording {
                    barrier: &mut run.context.barrier,
                    inbox: &mut run.context.inbox,
                    budget: &mut run.context.budget,
                    author: &run.context.author,
                })
                .unwrap(),
            AdmissionAdvance::Held
        );
        assert_eq!(
            run.context
                .take_classifier_admission(&mut run.clock)
                .unwrap(),
            Some(AdmissionPoll::Ready(ready))
        );
        if inventory.is_none() {
            run.context.complete_wait(PumpDeadline::Enter).unwrap();
        }
        Self { run, peer }
    }

    pub(super) fn records(&self, kind: Kind) -> Vec<serde_json::Value> {
        self.run
            .context
            .barrier
            .recorder()
            .structure()
            .by_kind(kind)
            .map(|record| serde_json::from_str(&record.line).unwrap())
            .collect()
    }

    pub(super) fn classifier_transport(&mut self) {
        self.run
            .context
            .flow
            .transport_one(
                &mut FlowRecording {
                    barrier: &mut self.run.context.barrier,
                    inbox: &mut self.run.context.inbox,
                    budget: &mut self.run.context.budget,
                    author: &self.run.context.author,
                },
                &mut self.run.clock,
                TransportSource::Classifier,
            )
            .unwrap();
    }

    pub(super) fn reply(&mut self, reply: &s::Ste100Reply) {
        self.peer
            .begin_send(
                Tag::Reply,
                reply.request_id.0,
                serde_json::to_vec(reply).unwrap(),
            )
            .unwrap();
        flush_peer(&mut self.peer);
        self.classifier_transport();
    }

    pub(super) fn no_request(&mut self) {
        assert_eq!(self.peer.try_receive().unwrap(), Receive::Pending);
    }

    fn no_turn(&self) {
        assert!(self.records(Kind::TurnStarted).is_empty());
        assert!(self.records(Kind::MessageUser).is_empty());
        assert!(self.records(Kind::TurnClosed).is_empty());
    }

    pub(super) fn begin(&mut self, frame: TurnFrame) {
        self.run
            .context
            .begin_input(frame, &mut self.run.clock)
            .unwrap();
    }

    fn advance(&mut self) -> Result<InputProgress, RunFailure> {
        self.run
            .context
            .advance_input(&mut ports(&mut self.run.clock))
    }

    fn request(&mut self) -> s::Ste100Request {
        for _ in 0..128 {
            self.advance().unwrap();
            self.classifier_transport();
            match self.peer.try_receive().unwrap() {
                Receive::Complete(message) => {
                    assert_eq!(message.tag, Tag::Request);
                    let request: s::Ste100Request =
                        s::decode_strict(std::str::from_utf8(&message.payload).unwrap()).unwrap();
                    assert_eq!(message.id, request.request_id.0);
                    return request;
                }
                Receive::Pending | Receive::Fragment => {}
            }
        }
        panic!("input check never crossed the actual classifier socket");
    }

    fn until(&mut self, expected: InputProgress) {
        for _ in 0..128 {
            if self.advance().unwrap() == expected {
                return;
            }
        }
        panic!("input did not reach {expected:?}");
    }

    pub(super) fn accepted_token(&mut self) -> (s::Ste100Request, s::Ste100Reply, AcceptedInput) {
        let request = self.request();
        let reply = accepted(&request);
        self.reply(&reply);
        assert!(
            self.run
                .context
                .take_accepted_input(&mut self.run.clock)
                .unwrap()
                .is_none()
        );
        assert!(self.records(Kind::Ste100Result).is_empty());
        self.until(InputProgress::Accepted);
        let token = self
            .run
            .context
            .take_accepted_input(&mut self.run.clock)
            .unwrap()
            .unwrap();
        assert!(
            self.run
                .context
                .take_accepted_input(&mut self.run.clock)
                .unwrap()
                .is_none()
        );
        (request, reply, token)
    }

    pub(super) fn fill_trace(&mut self) {
        let payload = serde_json::value::to_raw_value(&serde_json::json!({
            "fixture": "x".repeat(32_768)
        }))
        .unwrap();
        for _ in 0..weaver_trace::serving::ORDINARY_CAPACITY + 64 {
            if self.run.context.barrier.recorder().boundary().queued
                == weaver_trace::serving::ORDINARY_CAPACITY
            {
                return;
            }
            assert!(matches!(
                self.run.context.barrier.submit_authored_with_clock(
                    AuthoredEvent {
                        kind: Kind::ModelMeasurement,
                        subsystem: Subsystem::Harness,
                        turn: Some(TurnRef("pressure-fixture".into())),
                        source: super::super::author::AuthoredSource::Runtime,
                        causal_parent: None,
                        payload: Some(Payload::ModelMeasurement(payload.clone())),
                    },
                    RecordPurpose::Observation,
                    &self.run.context.author,
                    1,
                    &mut self.run.context.inbox,
                    &mut self.run.context.budget,
                    &mut || Ok(104),
                ),
                RecordProgress::Recorded { .. }
            ));
        }
        panic!("actual writer did not reach its ordinary queue capacity");
    }

    pub(super) fn drain_trace(&mut self) {
        self.run._reader.set_nonblocking(true).unwrap();
        let watchdog = Instant::now() + Duration::from_secs(3);
        let mut bytes = [0_u8; 65536];
        while self.run.context.barrier.recorder().boundary().queued != 0 {
            assert!(
                Instant::now() < watchdog,
                "actual trace writer did not drain"
            );
            match self.run._reader.read(&mut bytes) {
                Ok(0) => panic!("trace closed during pressure fixture"),
                Ok(_) => {}
                Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => {
                    std::thread::yield_now()
                }
                Err(error) => panic!("trace pressure drain: {error}"),
            }
        }
    }
}

fn accepted(request: &s::Ste100Request) -> s::Ste100Reply {
    assert!(request.text.starts_with("Read ") && request.text.ends_with('.'));
    let length = request.text.len() as u64;
    s::Ste100Reply {
        request_id: request.request_id,
        target: request.target.clone(),
        scheme_id: request.scheme_id.clone(),
        result: s::Ste100Result::Accepted {
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
        },
    }
}

#[test]
fn input_fixture_uses_the_real_open_load_and_classifier_admission() {
    let mut fx = InputFixture::new();
    assert_eq!(fx.run.context.phase(), RecordingPhase::Entered);
    assert_eq!(fx.records(Kind::Load).len(), 1);
    assert_eq!(fx.records(Kind::StateStoreMeasurement).len(), 1);
    assert!(fx.records(Kind::Ste100Result).is_empty());
    assert!(fx.run.context.flow.standing_classifier().is_some());
    assert!(fx.run.context.halt().is_none());
    fx.no_turn();
    fx.no_request();
}

#[test]
fn invalid_frames_record_original_offer_before_refusal_and_never_open_a_turn() {
    for frame in [
        TurnFrame {
            octets: "%%%=".into(),
            dialer: Some(42),
        },
        TurnFrame::carry_from(b"not JSON", 43),
        TurnFrame::carry_from(br#"{"text":"Read file.","role":"system"}"#, 42),
        TurnFrame::carry_from(br#"{"text":"Read file.","role":"user"}"#, 43),
        TurnFrame::carry_from(br#"{"text":"Read file.","role":null}"#, 42),
    ] {
        let mut fx = InputFixture::new();
        fx.begin(frame.clone());
        let initial = fx.run.context.input.snapshot().unwrap();
        assert_eq!(initial.state, InputState::Offered);
        assert!(initial.offered_sequence.is_none());
        assert!(fx.records(Kind::InputOffered).is_empty());
        assert!(fx.records(Kind::Refusal).is_empty());
        fx.no_request();
        assert_eq!(fx.advance().unwrap(), InputProgress::Progress);
        let offered = fx.records(Kind::InputOffered);
        assert_eq!(offered.len(), 1);
        assert_eq!(offered[0]["payload"]["octets"], frame.octets);
        assert_eq!(offered[0]["payload"]["input_id"], "1");
        assert_eq!(
            offered[0]["origin"],
            if frame.dialer == Some(42) {
                "operator"
            } else {
                "external"
            }
        );
        assert!(offered[0]["turn"].is_null());
        assert_eq!(
            fx.run.context.input.snapshot().unwrap().state,
            InputState::NeedRefusal
        );
        assert!(fx.records(Kind::Refusal).is_empty());
        fx.until(InputProgress::CloseReady);
        let refusals = fx.records(Kind::Refusal);
        assert_eq!(refusals.len(), 1);
        assert!(sequence(&offered[0]) < sequence(&refusals[0]));
        assert_eq!(refusals[0]["payload"]["seam"], "input");
        assert_eq!(refusals[0]["payload"]["input_id"], "1");
        assert_eq!(refusals[0]["payload"]["case"], "invalid_input_shape");
        assert_eq!(
            fx.run.context.input_close().unwrap(),
            TurnFrame::carry(br#"{"kind":"refused","reason":"Restate the input in STE100."}"#)
        );
        assert!(fx.run.context.deadlines.get(PumpDeadline::Gate).is_none());
        assert!(
            fx.run
                .context
                .take_accepted_input(&mut fx.run.clock)
                .unwrap()
                .is_none()
        );
        fx.no_turn();
        fx.no_request();
        fx.run
            .context
            .input_response_sent(&mut fx.run.clock)
            .unwrap();
        fx.run.context.finish_input(&mut fx.run.clock).unwrap();
        assert_eq!(fx.advance().unwrap(), InputProgress::Idle);
    }
}

fn sequence(record: &serde_json::Value) -> u64 {
    record["sequence"].as_str().unwrap().parse().unwrap()
}

#[test]
fn accepted_token_waits_for_the_exact_result_record_under_trace_pressure() {
    let mut fx = InputFixture::new();
    fx.begin(TurnFrame::carry_from(br#"{"text":"Read file."}"#, 42));
    let request = fx.request();
    assert_eq!(request.text, "Read file.");
    assert_eq!(
        request.target,
        s::Ste100Target::Input {
            input_id: 1.into(),
            block: 0.into()
        }
    );
    let bound = fx.run.context.deadlines.get(PumpDeadline::Gate).unwrap();
    while fx.run.context.flow.input_measurements_pending() {
        fx.advance().unwrap();
    }
    fx.fill_trace();
    let reply = accepted(&request);
    fx.reply(&reply);
    for _ in 0..128 {
        fx.advance().unwrap();
        if fx
            .run
            .context
            .pending_author()
            .is_some_and(|ticket| ticket.event.kind == Kind::Ste100Result)
        {
            break;
        }
    }
    let ticket = fx
        .run
        .context
        .pending_author()
        .expect("exact result author must wait");
    assert_eq!(ticket.event.kind, Kind::Ste100Result);
    let original_ticket = ticket.ticket;
    assert_eq!(
        fx.run.context.flow.standing_classifier().unwrap().held(),
        Some(&reply)
    );
    assert!(fx.records(Kind::Ste100Result).is_empty());
    assert!(
        fx.run
            .context
            .take_accepted_input(&mut fx.run.clock)
            .unwrap()
            .is_none()
    );
    assert_eq!(
        fx.run.context.input.snapshot().unwrap().state,
        InputState::Checking
    );
    assert_eq!(
        fx.run.context.deadlines.get(PumpDeadline::Gate),
        Some(bound)
    );
    fx.no_turn();
    fx.advance().unwrap();
    assert_eq!(
        fx.run.context.pending_author().unwrap().ticket,
        original_ticket
    );
    fx.drain_trace();
    fx.until(InputProgress::Accepted);
    let recorded = fx.records(Kind::Ste100Result);
    assert_eq!(recorded.len(), 1);
    assert_eq!(
        recorded[0]["payload"],
        serde_json::to_value(&reply).unwrap()
    );
    assert!(
        fx.run
            .context
            .take_accepted_input(&mut fx.run.clock)
            .unwrap()
            .is_some()
    );
    assert!(
        fx.run
            .context
            .take_accepted_input(&mut fx.run.clock)
            .unwrap()
            .is_none()
    );
    assert!(fx.run.context.deadlines.get(PumpDeadline::Gate).is_none());
    fx.no_turn();
    fx.no_request();
}

#[test]
fn consumption_authors_one_user_source_and_reuses_only_its_recorded_bindings() {
    let mut fx = InputFixture::new();
    let frame = TurnFrame::carry_from(br#"{"text":"Read caf\u00e9."}"#, 43);
    fx.begin(frame.clone());
    let (request, reply, token) = fx.accepted_token();
    assert_eq!(request.text, "Read caf\u{e9}.");
    let offered = fx.records(Kind::InputOffered);
    assert_eq!(offered[0]["payload"]["octets"], frame.octets);
    fx.no_turn();
    let turn = TurnRef("input-turn".into());
    let seated = fx
        .run
        .context
        .consume_input(token, turn.clone(), &mut ports(&mut fx.run.clock))
        .unwrap()
        .unwrap();
    assert_eq!(seated.message.role, Role::User);
    assert_eq!(
        seated.message.content,
        vec![ContentBlock::Text {
            text: request.text.clone()
        }]
    );
    assert_eq!(seated.input_id, 1.into());
    assert_eq!(seated.offered_sequence.0, sequence(&offered[0]));
    assert_eq!(seated.turn, turn);
    let starts = fx.records(Kind::TurnStarted);
    let users = fx.records(Kind::MessageUser);
    let results = fx.records(Kind::Ste100Result);
    assert_eq!((starts.len(), users.len(), results.len()), (1, 1, 1));
    assert!(sequence(&results[0]) < sequence(&starts[0]));
    assert!(sequence(&starts[0]) < sequence(&users[0]));
    assert_eq!(
        users[0]["payload"],
        serde_json::to_value(&seated.message).unwrap()
    );
    assert_eq!(users[0]["origin"], "external");
    assert_eq!(users[0]["causal_parent"], sequence(&offered[0]).to_string());
    assert_eq!(
        seated.source,
        s::SourceBlockId {
            run: "run".into(),
            sequence: sequence(&users[0]).into(),
            block: 0.into()
        }
    );
    let mut filing = None;
    for _ in 0..128 {
        if let Some(candidate) = fx.run.context.flow.ledger().next_file() {
            filing = Some(candidate);
            break;
        }
        fx.run.context.reduce_flow(&mut fx.run.clock).unwrap();
    }
    let LedgerFile::Statement(file) = filing.expect("selected user copy must become ready to file")
    else {
        panic!("input is a statement copy");
    };
    assert_eq!(file.block_id, seated.source);
    let s::Ste100Result::Accepted { bindings } = reply.result else {
        unreachable!();
    };
    assert_eq!(file.bindings, bindings);
    assert!(fx.run.context.input.snapshot().is_none());
    assert!(
        fx.run
            .context
            .take_accepted_input(&mut fx.run.clock)
            .unwrap()
            .is_none()
    );
    fx.no_request();
    assert_eq!(fx.records(Kind::MessageUser).len(), 1);
    assert_eq!(fx.records(Kind::TurnStarted).len(), 1);
}

#[test]
fn leave_before_submission_records_offer_then_closes_without_a_check() {
    let mut fx = InputFixture::new();
    let frame = TurnFrame::carry_from(br#"{"text":"Read file."}"#, 42);
    fx.begin(frame.clone());
    let bound = fx.run.context.deadlines.get(PumpDeadline::Gate).unwrap();
    fx.run.context.cancel_input(&mut fx.run.clock).unwrap();
    assert!(fx.run.context.input_close().is_none());
    fx.until(InputProgress::CloseReady);
    assert_eq!(
        fx.records(Kind::InputOffered)[0]["payload"]["octets"],
        frame.octets
    );
    assert_eq!(
        fx.run.context.input_close().unwrap(),
        TurnFrame::carry(br#"{"kind":"stopped","reason":"The agent is leaving."}"#)
    );
    assert_eq!(
        fx.run.context.deadlines.get(PumpDeadline::Gate),
        Some(bound)
    );
    fx.run
        .context
        .input_response_sent(&mut fx.run.clock)
        .unwrap();
    fx.until(InputProgress::Finished);
    assert!(fx.run.context.deadlines.get(PumpDeadline::Gate).is_none());
    assert!(
        fx.run
            .context
            .take_accepted_input(&mut fx.run.clock)
            .unwrap()
            .is_none()
    );
    fx.run.context.finish_input(&mut fx.run.clock).unwrap();
    assert!(fx.records(Kind::Ste100Result).is_empty());
    assert!(fx.records(Kind::Refusal).is_empty());
    fx.no_request();
    fx.no_turn();
}

#[test]
fn leave_after_submission_keeps_original_bound_and_records_late_acceptance_without_token() {
    let mut fx = InputFixture::new();
    fx.begin(TurnFrame::carry_from(br#"{"text":"Read file."}"#, 42));
    let request = fx.request();
    let bound = fx.run.context.deadlines.get(PumpDeadline::Gate).unwrap();
    fx.run.context.cancel_input(&mut fx.run.clock).unwrap();
    assert_eq!(
        fx.run.context.input_close().unwrap(),
        TurnFrame::carry(br#"{"kind":"stopped","reason":"The agent is leaving."}"#)
    );
    fx.run
        .context
        .input_response_sent(&mut fx.run.clock)
        .unwrap();
    assert_eq!(
        fx.run.context.deadlines.get(PumpDeadline::Gate),
        Some(bound)
    );
    assert!(!fx.run.context.input.snapshot().unwrap().result_recorded);
    assert!(
        fx.run
            .context
            .take_accepted_input(&mut fx.run.clock)
            .unwrap()
            .is_none()
    );
    let reply = accepted(&request);
    fx.reply(&reply);
    fx.until(InputProgress::Finished);
    assert_eq!(fx.records(Kind::Ste100Result).len(), 1);
    assert_eq!(
        fx.records(Kind::Ste100Result)[0]["payload"],
        serde_json::to_value(&reply).unwrap()
    );
    assert!(fx.run.context.input.snapshot().unwrap().result_recorded);
    assert!(fx.run.context.deadlines.get(PumpDeadline::Gate).is_none());
    assert!(
        fx.run
            .context
            .take_accepted_input(&mut fx.run.clock)
            .unwrap()
            .is_none()
    );
    fx.run.context.finish_input(&mut fx.run.clock).unwrap();
    fx.no_turn();
    fx.no_request();
}

#[test]
fn leave_revokes_an_already_transferred_token_without_authoring_a_turn() {
    let mut fx = InputFixture::new();
    fx.begin(TurnFrame::carry_from(br#"{"text":"Read file."}"#, 42));
    let (_, _, token) = fx.accepted_token();
    fx.run.context.cancel_input(&mut fx.run.clock).unwrap();
    fx.run
        .context
        .input_response_sent(&mut fx.run.clock)
        .unwrap();
    let error = fx
        .run
        .context
        .consume_input(
            token,
            TurnRef("forbidden-turn".into()),
            &mut ports(&mut fx.run.clock),
        )
        .unwrap_err();
    assert!(
        error.token.is_some(),
        "failed consumption must return the original capability"
    );
    assert!(matches!(
        error.failure,
        RunFailure::Flow {
            reason: FlowHaltReason::ProtocolFailed,
            ..
        }
    ));
    assert_eq!(fx.records(Kind::Ste100Result).len(), 1);
    fx.no_turn();
    fx.no_request();
}

#[test]
fn classifier_queue_delay_counts_against_the_original_gate_bound() {
    let mut fx = InputFixture::new();
    let template = s::TemplateSpec {
        template_id: "busy-template".into(),
        kind: "loop.instruction".into(),
        parts: vec![s::TemplatePart::Literal("Read earlier.".into())],
    };
    assert!(matches!(
        fx.run
            .context
            .flow
            .begin_check(
                &mut FlowRecording {
                    barrier: &mut fx.run.context.barrier,
                    inbox: &mut fx.run.context.inbox,
                    budget: &mut fx.run.context.budget,
                    author: &fx.run.context.author,
                },
                CheckOperation::Template {
                    template_id: "busy-template".into(),
                    text: serde_json::to_string(&template).unwrap()
                }
            )
            .unwrap(),
        OperationStart::Started { .. }
    ));
    fx.classifier_transport();
    let Receive::Complete(message) = fx.peer.try_receive().unwrap() else {
        panic!("earlier template request must cross the actual socket");
    };
    assert_eq!(message.tag, Tag::Request);
    let earlier: s::Ste100Request =
        s::decode_strict(std::str::from_utf8(&message.payload).unwrap()).unwrap();
    assert_eq!(
        earlier.target,
        s::Ste100Target::Template {
            template_id: "busy-template".into()
        }
    );
    fx.begin(TurnFrame::carry_from(br#"{"text":"Read waiting."}"#, 42));
    let bound = fx.run.context.deadlines.get(PumpDeadline::Gate).unwrap();
    fx.advance().unwrap();
    for _ in 0..8 {
        fx.advance().unwrap();
    }
    assert_eq!(
        fx.run.context.input.snapshot().unwrap().state,
        InputState::ReadyToCheck
    );
    fx.run.clock.advance_to(bound.deadline_ns() - 1).unwrap();
    fx.advance().unwrap();
    assert_eq!(
        fx.run.context.deadlines.get(PumpDeadline::Gate),
        Some(bound)
    );
    assert!(
        fx.run
            .context
            .input
            .snapshot()
            .unwrap()
            .request_id
            .is_none()
    );
    fx.run.clock.advance_to(bound.deadline_ns()).unwrap();
    assert!(matches!(
        fx.advance().unwrap_err(),
        RunFailure::Flow {
            reason: FlowHaltReason::GateBound,
            ..
        }
    ));
    let waited = fx.run.context.halt_wait().unwrap();
    assert_eq!(
        (waited.began_ns.0, waited.deadline_ns.0),
        (bound.began_ns(), bound.deadline_ns())
    );
    assert_eq!(
        fx.run.context.halt().unwrap().first().reason,
        FlowHaltReason::GateBound
    );
    assert_eq!(
        fx.run.context.input_close().unwrap(),
        TurnFrame::carry(
            br#"{"kind":"stopped","reason":"The agent stopped because state flow failed."}"#
        )
    );
    fx.no_turn();
    fx.no_request();
}

#[test]
fn offered_author_wait_uses_original_gate_bound_and_retains_unparsed_frame() {
    let mut fx = InputFixture::new();
    fx.fill_trace();
    let frame = TurnFrame {
        octets: "not canonical base64".into(),
        dialer: Some(42),
    };
    fx.begin(frame.clone());
    let bound = fx.run.context.deadlines.get(PumpDeadline::Gate).unwrap();
    assert!(matches!(
        fx.advance().unwrap_err(),
        RunFailure::Flow {
            reason: FlowHaltReason::GateBound,
            ..
        }
    ));
    let waited = fx.run.context.halt_wait().unwrap();
    assert_eq!(
        (waited.began_ns.0, waited.deadline_ns.0),
        (bound.began_ns(), bound.deadline_ns())
    );
    let ticket = fx
        .run
        .context
        .pending_author()
        .expect("original offer stays owned");
    assert_eq!(ticket.event.kind, Kind::InputOffered);
    let Some(Payload::InputOffered(offered)) = &ticket.event.payload else {
        panic!("offered payload");
    };
    assert_eq!(offered.octets, frame.octets);
    let input = fx.run.context.input.snapshot().unwrap();
    assert_eq!(input.state, InputState::Offered);
    assert!(input.halted);
    assert!(input.offered_sequence.is_none());
    assert!(input.request_id.is_none());
    assert!(fx.records(Kind::InputOffered).is_empty());
    assert!(fx.records(Kind::Refusal).is_empty());
    fx.no_turn();
    fx.no_request();
}

#[test]
fn non_ste100_records_result_then_refusal_before_unnamed_close() {
    let mut fx = InputFixture::new();
    fx.begin(TurnFrame::carry_from(br#"{"text":"Read file."}"#, 42));
    let request = fx.request();
    let reply = s::Ste100Reply {
        request_id: request.request_id,
        target: request.target,
        scheme_id: request.scheme_id,
        result: s::Ste100Result::NotSte100 {
            code: s::Ste100DefectCode::NonSte100,
            detail: "synthetic negative judgement".into(),
            spans: vec![],
        },
    };
    fx.reply(&reply);
    assert!(fx.run.context.input_close().is_none());
    assert!(
        fx.run
            .context
            .take_accepted_input(&mut fx.run.clock)
            .unwrap()
            .is_none()
    );
    fx.until(InputProgress::CloseReady);
    let result = fx.records(Kind::Ste100Result);
    let refusal = fx.records(Kind::Refusal);
    assert_eq!((result.len(), refusal.len()), (1, 1));
    assert_eq!(result[0]["payload"], serde_json::to_value(&reply).unwrap());
    assert!(sequence(&result[0]) < sequence(&refusal[0]));
    assert_eq!(
        refusal[0]["payload"],
        serde_json::json!({
            "seam": "input", "input_id": "1", "case": "non_ste100",
            "detail": "synthetic negative judgement"
        })
    );
    assert_eq!(
        fx.run.context.input_close().unwrap(),
        TurnFrame::carry(br#"{"kind":"refused","reason":"Restate the input in STE100."}"#)
    );
    assert!(fx.run.context.deadlines.get(PumpDeadline::Gate).is_none());
    fx.no_turn();
    fx.no_request();
}

#[test]
fn a_frame_queued_behind_another_input_keeps_its_bytes_without_starting_a_check() {
    let mut fx = InputFixture::new();
    fx.begin(TurnFrame::carry_from(b"bad first input", 42));
    let first = fx.run.context.input.snapshot().unwrap();
    let queued = TurnFrame::carry_from(br#"{"text":"Read next."}"#, 43);
    fx.run
        .clock
        .advance_to(first.bound.deadline_ns() - 1)
        .unwrap();
    let refused = fx
        .run
        .context
        .begin_input(queued.clone(), &mut fx.run.clock)
        .unwrap_err();
    assert_eq!(refused.frame, queued);
    assert!(refused.failure.is_none());
    assert_eq!(fx.run.context.input.snapshot(), Some(first));
    assert!(fx.records(Kind::InputOffered).is_empty());
    fx.until(InputProgress::CloseReady);
    fx.run
        .context
        .input_response_sent(&mut fx.run.clock)
        .unwrap();
    fx.run.context.finish_input(&mut fx.run.clock).unwrap();
    // The held caller frame gets a new anchor only at its own serialized intake.
    fx.run.clock.advance_to(2_000_104).unwrap();
    fx.begin(refused.frame);
    let second = fx.run.context.input.snapshot().unwrap();
    assert_eq!(second.input_id, 2.into());
    assert_eq!(second.bound.began_ns(), 2_000_104);
    assert_eq!(second.state, InputState::Offered);
    assert!(second.request_id.is_none());
    assert_eq!(fx.records(Kind::InputOffered).len(), 1);
    fx.no_turn();
    fx.no_request();
}

#[test]
fn timely_recorded_acceptance_can_be_taken_and_consumed_after_its_old_gate_bound() {
    let mut fx = InputFixture::new();
    fx.begin(TurnFrame::carry_from(br#"{"text":"Read file."}"#, 42));
    let request = fx.request();
    let bound = fx.run.context.input.snapshot().unwrap().bound;
    fx.reply(&accepted(&request));
    // Stop at the actual result record, before the queued completion notices.
    for _ in 0..128 {
        fx.advance().unwrap();
        if !fx.records(Kind::Ste100Result).is_empty() {
            break;
        }
    }
    assert_eq!(fx.records(Kind::Ste100Result).len(), 1);
    assert_eq!(
        fx.run.context.input.snapshot().unwrap().state,
        InputState::Accepted
    );
    assert!(fx.run.context.deadlines.get(PumpDeadline::Gate).is_none());
    fx.run.clock.advance_to(bound.deadline_ns() + 1).unwrap();
    fx.until(InputProgress::Accepted);
    let token = fx
        .run
        .context
        .take_accepted_input(&mut fx.run.clock)
        .unwrap()
        .unwrap();
    let seated = fx
        .run
        .context
        .consume_input(
            token,
            TurnRef("later-consumption".into()),
            &mut ports(&mut fx.run.clock),
        )
        .unwrap()
        .unwrap();
    assert_eq!(seated.input_id, 1.into());
    assert!(fx.run.context.failure().is_none());
    assert_eq!(fx.records(Kind::MessageUser).len(), 1);
    fx.no_request();
}

/// Advance the numeric clock precisely between preparing a result event and
/// sampling its Recorded boundary. This fixture performs no wait or socket I/O.
struct RecordBoundaryClock {
    before_reads: usize,
    before: u64,
    after: u64,
}

impl PumpIo for RecordBoundaryClock {
    fn now(&mut self) -> std::io::Result<u64> {
        if self.before_reads == 0 {
            Ok(self.after)
        } else {
            self.before_reads -= 1;
            Ok(self.before)
        }
    }
    fn wait(
        &mut self,
        _: &[super::super::clock::WaitInterest<'_>],
        _: Option<u64>,
    ) -> std::io::Result<super::super::clock::WaitOutcome> {
        panic!("record-boundary fixture must not wait");
    }
}

#[test]
fn acceptance_recorded_at_the_gate_bound_never_mints_a_token() {
    let mut fx = InputFixture::new();
    fx.begin(TurnFrame::carry_from(br#"{"text":"Read file."}"#, 42));
    let request = fx.request();
    while fx.run.context.flow.input_measurements_pending() {
        fx.advance().unwrap();
    }
    fx.reply(&accepted(&request));
    let bound = fx.run.context.input.snapshot().unwrap().bound;
    // reduce_flow checks live, advance captures now, then author captures its
    // envelope clock. The next read samples the completed result proof.
    let mut io = RecordBoundaryClock {
        before_reads: 3,
        before: 104,
        after: bound.deadline_ns(),
    };
    assert!(matches!(
        fx.run.context.reduce_flow(&mut io),
        Err(RunFailure::Flow {
            reason: FlowHaltReason::GateBound,
            ..
        })
    ));
    assert_eq!(
        fx.records(Kind::Ste100Result).len(),
        1,
        "the fixture must cross Recorded, not expire before authoring"
    );
    assert_eq!(
        fx.run.context.input.snapshot().unwrap().state,
        InputState::Checking
    );
    assert!(fx.run.context.input.snapshot().unwrap().halted);
    assert!(fx.run.context.take_accepted_input(&mut io).is_err());
    fx.no_turn();
}

fn external_input(
    fx: &InputFixture,
    input_id: Option<s::DecimalU64>,
) -> super::super::halt_snapshot::ExternalFacts {
    super::super::halt_snapshot::ExternalFacts {
        outstanding_pairs: vec![],
        capture: None,
        wait: fx.run.context.halt_wait().cloned(),
        input_id,
    }
}

#[test]
fn halt_account_refuses_an_input_identity_when_the_context_owns_no_input() {
    let mut fx = InputFixture::new();
    let bound = AbsoluteBound::after(104, 5).unwrap();
    fx.run
        .context
        .arm(PumpDeadline::Gate, bound, &mut fx.run.clock)
        .unwrap();
    fx.run.clock.advance_to(109).unwrap();
    assert!(fx.run.context.reduce_flow(&mut fx.run.clock).is_err());
    assert!(fx.run.context.input.snapshot().is_none());
    let forged = external_input(&fx, Some(99.into()));
    let HaltAccountFailure::Snapshot(rejected) =
        fx.run.context.freeze_halt_account(forged).unwrap_err()
    else {
        panic!("forged input must fail at the retained-owner check");
    };
    assert_eq!(rejected.external.input_id, Some(99.into()));
    assert!(fx.run.context.halt().unwrap().account().is_none());
    let actual = external_input(&fx, None);
    fx.run.context.freeze_halt_account(actual).unwrap();
    assert!(
        fx.run
            .context
            .halt()
            .unwrap()
            .account()
            .unwrap()
            .input_id
            .is_none()
    );
}

#[test]
fn halt_account_keeps_queued_input_and_the_distinct_source_classifier_request() {
    let mut fx = InputFixture::new();
    let authored = AuthoredEvent {
        kind: Kind::MessageAssistant,
        subsystem: Subsystem::Harness,
        turn: Some(TurnRef("earlier-turn".into())),
        source: super::super::author::AuthoredSource::Model,
        causal_parent: Some(Sequence(0)),
        payload: Some(Payload::Message(
            serde_json::value::to_raw_value(&serde_json::json!({
                "role": "assistant", "content": [{"type": "text", "text": "Read earlier."}]
            }))
            .unwrap(),
        )),
    };
    let source_sequence = fx.run.author(authored).unwrap();
    let source = s::SourceBlockId {
        run: "run".into(),
        sequence: source_sequence.0.into(),
        block: 0.into(),
    };
    let mut request = None;
    for _ in 0..128 {
        fx.run.context.reduce_flow(&mut fx.run.clock).unwrap();
        fx.classifier_transport();
        if let Receive::Complete(message) = fx.peer.try_receive().unwrap() {
            assert_eq!(message.tag, Tag::Request);
            request = Some(
                s::decode_strict::<s::Ste100Request>(
                    std::str::from_utf8(&message.payload).unwrap(),
                )
                .unwrap(),
            );
            break;
        }
    }
    let request = request.expect("selected source check crosses the classifier socket");
    assert_eq!(request.target, s::Ste100Target::Source(source.clone()));
    fx.begin(TurnFrame::carry_from(br#"{"text":"Read queued."}"#, 43));
    fx.advance().unwrap();
    assert_eq!(
        fx.run.context.input.snapshot().unwrap().state,
        InputState::ReadyToCheck
    );
    let mut wrong = accepted(&request);
    wrong.request_id = (request.request_id.0 + 1).into();
    fx.peer
        .begin_send(
            Tag::Reply,
            wrong.request_id.0,
            serde_json::to_vec(&wrong).unwrap(),
        )
        .unwrap();
    flush_peer(&mut fx.peer);
    assert!(
        fx.run
            .context
            .flow
            .transport_one(
                &mut FlowRecording {
                    barrier: &mut fx.run.context.barrier,
                    inbox: &mut fx.run.context.inbox,
                    budget: &mut fx.run.context.budget,
                    author: &fx.run.context.author,
                },
                &mut fx.run.clock,
                TransportSource::Classifier
            )
            .is_err()
    );
    assert!(fx.advance().is_err());
    let actual = external_input(&fx, Some(1.into()));
    fx.run.context.freeze_halt_account(actual).unwrap();
    let account = fx.run.context.halt().unwrap().account().unwrap();
    assert_eq!(account.input_id, Some(1.into()));
    assert_eq!(account.source_id.as_ref(), Some(&source));
    let retained = account.request.as_ref().unwrap();
    assert_eq!(retained.domain, super::super::report::RequestDomain::Ste100);
    assert_eq!(retained.id, request.request_id);
    assert_eq!(account.outstanding_blocks.len(), 1);
    assert_eq!(account.outstanding_blocks[0].block_id, source);
    assert!(fx.records(Kind::MessageUser).is_empty());
    assert!(fx.records(Kind::Ste100Result).is_empty());
}

#[test]
fn an_input_component_failure_records_exact_result_before_halt_and_stopped_close() {
    for code in [
        s::Ste100ComponentCode::Crashed,
        s::Ste100ComponentCode::NoResult,
        s::Ste100ComponentCode::InvalidResult,
        s::Ste100ComponentCode::TransportFailed,
    ] {
        let mut fx = InputFixture::new();
        fx.begin(TurnFrame::carry_from(br#"{"text":"Read file."}"#, 42));
        let request = fx.request();
        let reply = s::Ste100Reply {
            request_id: request.request_id,
            target: request.target,
            scheme_id: request.scheme_id,
            result: s::Ste100Result::ComponentFailed {
                code,
                detail: format!("exact fixture {code:?}"),
            },
        };
        fx.reply(&reply);
        fx.peer.close();
        assert!(fx.records(Kind::Ste100Result).is_empty());
        assert!(fx.run.context.input_close().is_none());
        let mut failed = None;
        for _ in 0..128 {
            match fx.advance() {
                Ok(_) => {}
                Err(error) => {
                    failed = Some(error);
                    break;
                }
            }
        }
        assert!(matches!(
            failed,
            Some(RunFailure::Flow {
                reason: FlowHaltReason::ClassifierFailed,
                ..
            })
        ));
        let result = fx.records(Kind::Ste100Result);
        assert_eq!(result.len(), 1);
        assert_eq!(result[0]["payload"], serde_json::to_value(&reply).unwrap());
        assert_eq!(
            fx.run.context.input_close().unwrap(),
            TurnFrame::carry(
                br#"{"kind":"stopped","reason":"The agent stopped because state flow failed."}"#
            )
        );
        assert!(
            fx.run
                .context
                .take_accepted_input(&mut fx.run.clock)
                .is_err()
        );
        assert!(fx.records(Kind::Refusal).is_empty());
        assert!(fx.run.context.input.snapshot().unwrap().halted);
        fx.no_turn();
        fx.run
            .context
            .input_response_sent(&mut fx.run.clock)
            .unwrap();
        assert!(
            fx.run.context.input_close().is_none(),
            "the stopped answer transfers once"
        );
    }
}

#[test]
fn a_leave_bound_interrupts_after_turn_started_and_resumes_the_same_source_handoff() {
    let mut fx = InputFixture::new();
    fx.begin(TurnFrame::carry_from(br#"{"text":"Read file."}"#, 42));
    let (request, _, token) = fx.accepted_token();
    let bound = AbsoluteBound::after(104, 5).unwrap();
    fx.run
        .context
        .arm(PumpDeadline::Leave, bound, &mut fx.run.clock)
        .unwrap();
    // consume checks live, resume checks live, author checks live and captures
    // the event clock. Its post-Recorded check then sees the lifecycle bound.
    let mut io = RecordBoundaryClock {
        before_reads: 4,
        before: 104,
        after: 109,
    };
    let error = fx
        .run
        .context
        .consume_input(
            token,
            TurnRef("interrupted-input".into()),
            &mut ports(&mut io),
        )
        .unwrap_err();
    assert!(
        error.token.is_none(),
        "the context retains the consumed handoff"
    );
    assert_eq!(
        error.failure,
        RunFailure::Wait {
            key: PumpDeadline::Leave,
            bound
        }
    );
    assert_eq!(fx.records(Kind::TurnStarted).len(), 1);
    assert!(fx.records(Kind::MessageUser).is_empty());
    assert!(fx.run.context.input_turn.is_some());
    assert_eq!(
        fx.run.context.input.snapshot().unwrap().state,
        InputState::Consumed
    );
    assert!(fx.run.context.completed_author.is_some());
    assert!(fx.run.context.failure().is_none());
    fx.run.context.complete_wait(PumpDeadline::Leave).unwrap();
    fx.run.clock.advance_to(109).unwrap();
    let seated = fx
        .run
        .context
        .resume_input_turn(&mut ports(&mut fx.run.clock))
        .unwrap()
        .unwrap();
    assert_eq!(
        seated.message.content,
        vec![ContentBlock::Text { text: request.text }]
    );
    assert_eq!(fx.records(Kind::TurnStarted).len(), 1);
    assert_eq!(fx.records(Kind::MessageUser).len(), 1);
    assert!(fx.run.context.input_turn.is_none());
    assert!(fx.run.context.completed_author.is_none());
    assert!(fx.run.context.input.snapshot().is_none());
    assert_eq!(
        seated.source.sequence.0,
        sequence(&fx.records(Kind::MessageUser)[0])
    );
    fx.no_request();
}

#[test]
fn early_finish_refuses_an_interrupted_source_handoff_without_losing_input_identity() {
    let mut fx = InputFixture::new();
    fx.begin(TurnFrame::carry_from(br#"{"text":"Read file."}"#, 42));
    let (_, _, token) = fx.accepted_token();
    let bound = AbsoluteBound::after(104, 5).unwrap();
    fx.run
        .context
        .arm(PumpDeadline::Leave, bound, &mut fx.run.clock)
        .unwrap();
    let mut io = RecordBoundaryClock {
        before_reads: 4,
        before: 104,
        after: 109,
    };
    let interrupted = fx
        .run
        .context
        .consume_input(
            token,
            TurnRef("unfinished-input".into()),
            &mut ports(&mut io),
        )
        .unwrap_err();
    assert!(interrupted.token.is_none());
    assert_eq!(
        interrupted.failure,
        RunFailure::Wait {
            key: PumpDeadline::Leave,
            bound,
        }
    );
    assert_eq!(fx.records(Kind::TurnStarted).len(), 1);
    assert!(fx.records(Kind::MessageUser).is_empty());
    assert!(fx.run.context.input_turn.is_some());
    let completed_author = fx.run.context.completed_author;
    assert!(completed_author.is_some());
    let mut expected_input = fx.run.context.input.snapshot().unwrap();
    assert_eq!(expected_input.state, InputState::Consumed);
    assert!(fx.run.context.input_author.is_none());
    assert!(!fx.run.context.flow.input_measurements_pending());

    // Dispose only the lifecycle interruption, leaving the source handoff owed.
    fx.run.context.complete_wait(PumpDeadline::Leave).unwrap();
    fx.run.clock.advance_to(109).unwrap();
    assert!(matches!(
        fx.run.context.finish_input(&mut fx.run.clock),
        Err(RunFailure::Flow {
            reason: FlowHaltReason::ProtocolFailed,
            ..
        })
    ));
    expected_input.halted = true;
    assert_eq!(fx.run.context.input.snapshot(), Some(expected_input));
    assert!(fx.run.context.input_turn.is_some());
    assert_eq!(fx.run.context.completed_author, completed_author);
    assert_eq!(fx.records(Kind::TurnStarted).len(), 1);
    assert!(fx.records(Kind::MessageUser).is_empty());
    assert!(fx.run.context.input_close().is_none());
    let actual = external_input(&fx, Some(1.into()));
    fx.run.context.freeze_halt_account(actual).unwrap();
    assert_eq!(
        fx.run.context.halt().unwrap().account().unwrap().input_id,
        Some(1.into())
    );
    fx.no_request();
}

struct FailInputRegistrationClock {
    reads: usize,
}

impl PumpIo for FailInputRegistrationClock {
    fn now(&mut self) -> std::io::Result<u64> {
        self.reads += 1;
        if self.reads == 2 {
            Err(std::io::Error::other("fixture registration clock failed"))
        } else {
            Ok(104)
        }
    }
    fn wait(
        &mut self,
        _: &[super::super::clock::WaitInterest<'_>],
        _: Option<u64>,
    ) -> std::io::Result<super::super::clock::WaitOutcome> {
        panic!("registration must not wait");
    }
}

#[test]
fn a_clock_failure_after_frame_intake_freezes_one_pending_input_before_any_offer() {
    let mut fx = InputFixture::new();
    let frame = TurnFrame::carry_from(br#"{"text":"Read file."}"#, 42);
    let mut io = FailInputRegistrationClock { reads: 0 };
    let error = fx
        .run
        .context
        .begin_input(frame.clone(), &mut io)
        .unwrap_err();
    assert_eq!(error.frame, frame);
    assert!(matches!(
        error.failure,
        Some(RunFailure::Flow {
            reason: FlowHaltReason::TraceFailed,
            ..
        })
    ));
    let input = fx.run.context.input.snapshot().unwrap();
    assert_eq!(input.input_id, 1.into());
    assert!(input.halted);
    assert!(input.offered_sequence.is_none());
    assert!(input.request_id.is_none());
    let actual = external_input(&fx, Some(1.into()));
    fx.run.context.freeze_halt_account(actual).unwrap();
    let account = fx.run.context.halt().unwrap().account().unwrap();
    assert_eq!(account.input_id, Some(1.into()));
    assert_eq!(account.flow_sample.input_pending.0, 1);
    assert_eq!(account.flow_sample.input_started.0, 1);
    assert_eq!(account.flow_sample.input_completed.0, 0);
    assert!(fx.records(Kind::InputOffered).is_empty());
    assert!(fx.records(Kind::Ste100Result).is_empty());
    fx.no_turn();
    fx.no_request();
}
