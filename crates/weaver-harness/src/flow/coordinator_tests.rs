//! The coordinator uses real trace admission and the sole real state socket owner.
//! The scripted classifier and state peer make no production model/store claim.

use super::*;
use crate::flow::channel::StateRequest;
use crate::flow::classifier::scripted::{ScriptStep, ScriptedPeer, ScriptedPort};
use crate::flow::clock::testing::FakePumpIo;
use std::io::{BufRead, BufReader, Read, Write};
use std::os::unix::net::UnixStream;
use std::time::{Duration, Instant};
use weaver_trace::serving::{
    ORDINARY_CAPACITY, PreflightAttempt, PreflightRecorder, validate_partition,
};
use weaver_trace::{RunRef, SessionRef};
use weaver_types::state_flow as s;

use crate::flow::recording_fixture as fixture;

pub(in crate::flow) struct Fixture {
    pub(in crate::flow) flow: FlowCoordinator<ScriptedPort>,
    pub(in crate::flow) classifier: ScriptedPeer,
    pub(in crate::flow) state_peer: UnixStream,
    state_bytes: Vec<u8>,
    pub(in crate::flow) requests: Vec<s::StateFrame>,
    classifier_replies: usize,
    operation: u64,
    pub(in crate::flow) barrier: RecordBarrier,
    pub(in crate::flow) inbox: FlowInbox,
    pub(in crate::flow) budget: RetentionBudget,
    pub(in crate::flow) author: ServingAuthor,
    pub(in crate::flow) clock: FakePumpIo,
    reader: Option<UnixStream>,
}

fn authored(kind: Kind, source: AuthoredSource, payload: Payload, turn: bool) -> AuthoredEvent {
    AuthoredEvent {
        kind,
        subsystem: Subsystem::Harness,
        turn: turn.then(|| TurnRef("turn".into())),
        source,
        causal_parent: None,
        payload: Some(payload),
    }
}

pub(in crate::flow) fn operation(
    request: &s::StateFrame,
    id: u64,
    began: u64,
) -> s::StateOperationObservation {
    s::StateOperationObservation {
        operation_id: id.into(),
        parent_id: None,
        request_id: request.common().request_id,
        operation: request.request_operation().unwrap(),
        began_ns: began.into(),
        ended_ns: (began + 1).into(),
        phases: vec![
            s::OperationPhase {
                name: s::PhaseName::Validate,
                began_ns: began.into(),
                ended_ns: began.into(),
            },
            s::OperationPhase {
                name: s::PhaseName::Encode,
                began_ns: (began + 1).into(),
                ended_ns: (began + 1).into(),
            },
        ],
        resource: s::OperationResource {
            cpu_ns: None,
            copied_bytes: Some(0.into()),
            held_bytes: Some(1.into()),
            unavailable: vec![s::ResourceUnavailable {
                field: s::ResourceField::CpuNs,
                reason: s::ObservationUnavailable::NotSampled,
            }],
        },
        disk: s::OperationDisk {
            enabled: None,
            before: None,
            after: None,
            unavailable_reason: Some(s::ObservationUnavailable::NotSampled),
        },
    }
}

fn accepted(request: &s::Ste100Request) -> s::Ste100Reply {
    assert!(request.text.starts_with("Read ") && request.text.ends_with('.'));
    let len = request.text.len() as u64;
    s::Ste100Reply {
        request_id: request.request_id,
        target: request.target.clone(),
        scheme_id: request.scheme_id.clone(),
        result: s::Ste100Result::Accepted {
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
        },
    }
}

impl Fixture {
    pub(in crate::flow) fn new(max_blocks: u64) -> Self {
        let author = ServingAuthor::new("session".into(), "run".into(), 0, 100).unwrap();
        let (sink, reader) = UnixStream::pair().unwrap();
        sink.set_write_timeout(Some(Duration::from_secs(10)))
            .unwrap();
        reader
            .set_read_timeout(Some(Duration::from_secs(10)))
            .unwrap();
        let recorder = PreflightRecorder::receive(
            sink.into(),
            RunRef("run".into()),
            SessionRef("session".into()),
        )
        .unwrap();
        let load = author
            .prepare(
                &authored(
                    Kind::Load,
                    AuthoredSource::Runtime,
                    Payload::Elections(Box::new(fixture::load_record())),
                    false,
                ),
                100,
                1,
            )
            .unwrap();
        let PreflightAttempt::Recorded { recorder, .. } = recorder
            .record_load(load)
            .unwrap_or_else(|e| panic!("load: {:?}", e.failure))
        else {
            panic!("empty trace capacity");
        };
        let mut barrier = RecordBarrier::new(
            recorder.activate(validate_partition(&fixture::partition()).unwrap()),
            "run".into(),
        );
        let mut inbox = FlowInbox::new();
        let mut budget = RetentionBudget::new(s::FlowBacklog {
            max_pending_blocks: max_blocks.into(),
            max_pending_bytes: 100000.into(),
            lag_alarm_turns: 2.into(),
            measurement_ref: "test-only capacity".into(),
        })
        .unwrap();
        let (socket, mut peer) = UnixStream::pair().unwrap();
        peer.set_read_timeout(Some(Duration::from_secs(3))).unwrap();
        peer.set_write_timeout(Some(Duration::from_secs(3)))
            .unwrap();
        let mut state = StateChannel::new(socket, "session".into(), "run".into()).unwrap();
        state
            .begin(
                StateRequest::Open {
                    schema_id: s::STATE_CONTENT_SCHEMA.into(),
                    scheme_id: "fixture-read/1".into(),
                    start: serde_json::from_value(
                        serde_json::to_value(fixture::load_record().state_start).unwrap(),
                    )
                    .unwrap(),
                    read_limits: s::ReadLimits {
                        max_result_bytes: 100000.into(),
                        measurement_ref: "test-only reads".into(),
                    },
                },
                100,
            )
            .unwrap();
        state.progress(&mut || Ok(110)).unwrap();
        let mut bytes = Vec::new();
        BufReader::new(&mut peer)
            .read_until(b'\n', &mut bytes)
            .unwrap();
        let request = s::decode_state_line(&bytes, s::FrameDirection::Request).unwrap();
        let observation = operation(&request, 1, 111);
        let s::StateFrame::Open {
            common,
            schema_id,
            scheme_id,
            start,
            read_limits,
        } = request
        else {
            panic!("open request");
        };
        let answer = s::StateFrame::Opened {
            common,
            schema_id,
            scheme_id,
            start,
            read_limits,
            observation,
            member_observation: s::MemberObservationIdentity {
                instrument_version: "state-observation/2".into(),
                pid: 7,
                start_ticks: Some(10.into()),
                ticks_per_second: Some(100.into()),
                unavailable_reason: None,
            },
        };
        peer.write_all(&s::encode_state_line(&answer, s::FrameDirection::Response).unwrap())
            .unwrap();
        state.progress(&mut || Ok(120)).unwrap();
        let measurement = state.held_measurement().unwrap().clone();
        let event = author
            .prepare(
                &authored(
                    Kind::StateStoreMeasurement,
                    AuthoredSource::Runtime,
                    Payload::StateStoreMeasurement(Box::new(super::super::mirror::measurement(
                        &measurement,
                    ))),
                    false,
                ),
                120,
                1,
            )
            .unwrap();
        let RecordProgress::Recorded {
            proof: Some(RecordProof::State(proof)),
            ..
        } = barrier.submit_with_clock(
            event,
            RecordPurpose::State(measurement),
            &mut inbox,
            &mut budget,
            &mut || Ok(120),
        )
        else {
            panic!("actual open measurement admission");
        };
        state.release(proof).unwrap();
        peer.set_nonblocking(true).unwrap();
        let (port, classifier) = ScriptedPort::fixture([]).unwrap();
        let observations = Observation::new(
            "run".into(),
            120,
            s::MeasuredInterval {
                elapsed_ns: 1000000.into(),
                measurement_ref: "test-only cadence".into(),
            },
            2,
        )
        .unwrap();
        let flow = FlowCoordinator::new(
            "run".into(),
            Ledger::fixture("run".into(), "session".into()),
            Classifier::fixture(port, "run".into()),
            state,
            observations,
        )
        .unwrap();
        Self {
            flow,
            classifier,
            state_peer: peer,
            state_bytes: Vec::new(),
            requests: Vec::new(),
            classifier_replies: 0,
            operation: 2,
            barrier,
            inbox,
            budget,
            author,
            clock: FakePumpIo::new(130),
            reader: Some(reader),
        }
    }

    pub(in crate::flow) fn time(&mut self) -> u64 {
        self.clock.now().unwrap()
    }
    pub(in crate::flow) fn elapse(&mut self, delta: u64) {
        let now = self.time();
        self.clock.advance_to(now + delta).unwrap();
    }

    pub(in crate::flow) fn advance(&mut self) -> Result<Advance, FlowFailure> {
        self.elapse(1);
        self.flow.advance(
            &mut FlowRecording {
                barrier: &mut self.barrier,
                inbox: &mut self.inbox,
                budget: &mut self.budget,
                author: &self.author,
            },
            &mut self.clock,
            1,
        )
    }

    pub(in crate::flow) fn transport(&mut self) -> Result<(), FlowFailure> {
        use nix::poll::{PollFd, PollFlags, PollTimeout, poll};
        let classifier_ready = {
            let mut fds = [PollFd::new(
                self.flow.classifier().readiness(),
                PollFlags::POLLIN,
            )];
            poll(&mut fds, PollTimeout::ZERO).unwrap();
            fds[0].revents().is_some_and(|flags| !flags.is_empty())
        };
        self.elapse(1);
        self.flow.transport(
            &mut FlowRecording {
                barrier: &mut self.barrier,
                inbox: &mut self.inbox,
                budget: &mut self.budget,
                author: &self.author,
            },
            &mut self.clock,
            true,
            classifier_ready,
        )
    }

    pub(in crate::flow) fn admit(&mut self, texts: &[&str]) -> Vec<SourceBlockId> {
        self.elapse(1);
        let payload = serde_json::json!({"role":"assistant","content":texts.iter().map(|text|serde_json::json!({"type":"text","text":text})).collect::<Vec<_>>()});
        let raw = serde_json::value::to_raw_value(&payload).unwrap();
        let now = self.time();
        let event = self
            .author
            .prepare(
                &authored(
                    Kind::MessageAssistant,
                    AuthoredSource::Model,
                    Payload::Message(raw),
                    true,
                ),
                now,
                1,
            )
            .unwrap();
        let RecordProgress::Recorded { sequence, .. } = self.barrier.submit_with_clock(
            event,
            RecordPurpose::Content,
            &mut self.inbox,
            &mut self.budget,
            &mut || Ok(now),
        ) else {
            panic!("content admission");
        };
        (0..texts.len())
            .map(|block| SourceBlockId {
                run: "run".into(),
                sequence: sequence.0.into(),
                block: (block as u64).into(),
            })
            .collect()
    }

    pub(in crate::flow) fn collect_requests(&mut self) {
        let mut chunk = [0; 8192];
        loop {
            match self.state_peer.read(&mut chunk) {
                Ok(0) => panic!("state channel closed"),
                Ok(count) => self.state_bytes.extend_from_slice(&chunk[..count]),
                Err(error) if error.kind() == io::ErrorKind::WouldBlock => break,
                Err(error) => panic!("state peer read: {error}"),
            }
        }
        while let Some(end) = self.state_bytes.iter().position(|byte| *byte == b'\n') {
            let frame: Vec<_> = self.state_bytes.drain(..=end).collect();
            self.requests
                .push(s::decode_state_line(&frame, s::FrameDirection::Request).unwrap());
        }
    }

    pub(in crate::flow) fn answer_classifier(&mut self, reject: bool) {
        let submitted = self.classifier.submitted();
        if self.classifier_replies == submitted.len() {
            return;
        }
        assert_eq!(submitted.len(), self.classifier_replies + 1);
        let request = &submitted[self.classifier_replies];
        let mut reply = accepted(request);
        if reject {
            reply.result = s::Ste100Result::NotSte100 {
                code: s::Ste100DefectCode::NonSte100,
                detail: "test-only conformance refusal".into(),
                spans: Vec::new(),
            };
        }
        self.classifier.push(ScriptStep::Reply(reply));
        assert!(self.classifier.advance().unwrap());
        self.classifier_replies += 1;
    }

    pub(in crate::flow) fn answer_file(&mut self, index: usize, wrong_identity: bool) {
        let now = self.time();
        let request = &self.requests[index];
        let s::StateFrame::File { common, filing } = request else {
            panic!("exact statement filing request");
        };
        let mut common = common.clone();
        if wrong_identity {
            common.request_id = (common.request_id.0 + 1).into();
        }
        let mut observation = operation(request, self.operation, now + 1);
        observation.request_id = common.request_id;
        let answer = s::StateFrame::Filed {
            common,
            block_id: filing.block_id.clone(),
            observation,
        };
        self.operation += 1;
        self.state_peer
            .write_all(&s::encode_state_line(&answer, s::FrameDirection::Response).unwrap())
            .unwrap();
        self.elapse(3);
    }

    fn drive(&mut self, reject_first: bool) {
        let mut answered = self.requests.len();
        for _ in 0..256 {
            self.transport().unwrap();
            let progress = self.advance().unwrap();
            self.answer_classifier(reject_first && self.classifier_replies == 0);
            self.collect_requests();
            while answered < self.requests.len() {
                self.answer_file(answered, false);
                answered += 1;
            }
            if progress == Advance::Idle
                && self.flow.ledger().facts().next().is_none()
                && self.budget.waiting_copy().is_none()
            {
                return;
            }
        }
        panic!("bounded fixture did not finish ordered filing");
    }

    pub(in crate::flow) fn until_file(&mut self) {
        for _ in 0..128 {
            self.transport().unwrap();
            self.advance().unwrap();
            self.answer_classifier(false);
            self.collect_requests();
            if !self.requests.is_empty() {
                return;
            }
        }
        panic!("filing request did not reach real socket");
    }

    pub(in crate::flow) fn until_classifier_request(&mut self) -> s::Ste100Request {
        for _ in 0..128 {
            self.transport().unwrap();
            self.advance().unwrap();
            let requests = self.classifier.submitted();
            if let Some(request) = requests.first() {
                return request.clone();
            }
        }
        panic!("classifier request did not reach its port");
    }

    pub(super) fn fill_trace_capacity(&mut self) -> String {
        let large = "x".repeat(32768);
        for _ in 0..ORDINARY_CAPACITY + 64 {
            let now = self.time();
            let event = self
                .author
                .prepare(
                    &authored(
                        Kind::ModelMeasurement,
                        AuthoredSource::Runtime,
                        Payload::ModelMeasurement(
                            serde_json::value::to_raw_value(&serde_json::json!({"fixture":large}))
                                .unwrap(),
                        ),
                        true,
                    ),
                    now,
                    1,
                )
                .unwrap();
            match self.barrier.submit_with_clock(
                event,
                RecordPurpose::Observation,
                &mut self.inbox,
                &mut self.budget,
                &mut || Ok(now),
            ) {
                RecordProgress::Recorded { .. } => {}
                RecordProgress::Pending => {
                    return serde_json::to_string(self.barrier.pending().unwrap()).unwrap();
                }
                other => panic!(
                    "fill trace: {other:?}, failure: {:?}",
                    self.barrier.failure()
                ),
            }
        }
        panic!("bounded fixture did not reach real trace capacity");
    }

    pub(super) fn resume_external_ticket(&mut self) {
        let deadline = Instant::now() + Duration::from_secs(8);
        loop {
            let now = self.time();
            match self
                .barrier
                .resume_with_clock(&mut self.inbox, &mut self.budget, &mut || Ok(now))
            {
                RecordProgress::Recorded { .. } => return,
                RecordProgress::Pending => {
                    assert!(Instant::now() < deadline);
                    std::thread::yield_now();
                }
                other => panic!("pending author: {other:?}"),
            }
        }
    }

    fn records(&self) -> Vec<serde_json::Value> {
        self.barrier
            .recorder()
            .structure()
            .iter()
            .map(|record| serde_json::from_str(&record.line).unwrap())
            .collect()
    }

    pub(super) fn start_drain(&mut self) -> std::thread::JoinHandle<Vec<u8>> {
        let mut reader = self.reader.take().unwrap();
        std::thread::spawn(move || {
            let mut bytes = Vec::new();
            reader.read_to_end(&mut bytes).unwrap();
            bytes
        })
    }
}

#[test]
fn actual_records_and_socket_receipts_preserve_source_and_block_filing_order() {
    let mut fx = Fixture::new(8);
    let mut sources = fx.admit(&["Read first.", "Read caf\u{e9}."]);
    sources.extend(fx.admit(&["Read third."]));
    fx.drive(false);
    let filings = fx
        .requests
        .iter()
        .map(|frame| match frame {
            s::StateFrame::File { filing, .. } => filing,
            _ => panic!("file"),
        })
        .collect::<Vec<_>>();
    assert_eq!(
        filings
            .iter()
            .map(|file| &file.block_id)
            .collect::<Vec<_>>(),
        sources.iter().collect::<Vec<_>>()
    );
    assert_eq!(
        filings
            .iter()
            .map(|file| file.text.as_str())
            .collect::<Vec<_>>(),
        ["Read first.", "Read caf\u{e9}.", "Read third."]
    );
    assert_eq!(
        filings[1].source_path,
        vec![
            s::SourcePathPart::Key("content".into()),
            s::SourcePathPart::Index { index: 1.into() }
        ]
    );
    assert_eq!(filings[1].bindings[0].nouns[0].value, "caf\u{e9}");
    assert!(fx.flow.ledger().barrier(&sources).unwrap());
    assert_eq!(fx.budget.gauges(), (0, 0));
    assert!(fx.flow.failure().is_none());
    let records = fx.records();
    let flow = records
        .iter()
        .filter(|record| record["kind"] == "state.flow")
        .collect::<Vec<_>>();
    for source in &sources {
        let sequence = source.sequence.0.to_string();
        let block = source.block.0.to_string();
        let same = |record: &&&serde_json::Value| {
            record["payload"]["source_id"]["sequence"].as_str() == Some(sequence.as_str())
                && record["payload"]["source_id"]["block"].as_str() == Some(block.as_str())
        };
        let phases = flow
            .iter()
            .filter(same)
            .map(|record| record["payload"]["phase"].as_str().unwrap())
            .collect::<Vec<_>>();
        assert_eq!(
            phases,
            [
                "copied",
                "classification_started",
                "accepted",
                "file_started",
                "filed"
            ]
        );
        let result = records
            .iter()
            .position(|record| {
                record["kind"] == "ste100.result"
                    && record["payload"]["target"]["source"]["sequence"].as_str()
                        == Some(sequence.as_str())
                    && record["payload"]["target"]["source"]["block"].as_str()
                        == Some(block.as_str())
            })
            .unwrap();
        let receipt = records
            .iter()
            .position(|record| {
                record["kind"] == "state.store.measurement"
                    && record["payload"]["request_source"]["sequence"].as_str()
                        == Some(sequence.as_str())
                    && record["payload"]["request_source"]["block"].as_str() == Some(block.as_str())
            })
            .unwrap();
        let filed = records
            .iter()
            .position(|record| {
                record["kind"] == "state.flow"
                    && record["payload"]["phase"] == "filed"
                    && record["payload"]["source_id"]["sequence"].as_str()
                        == Some(sequence.as_str())
                    && record["payload"]["source_id"]["block"].as_str() == Some(block.as_str())
            })
            .unwrap();
        assert!(
            result < receipt && receipt < filed,
            "recorded classifier and operation evidence precede terminal release"
        );
        let measurement = &records[receipt]["payload"];
        let terminal = &records[filed]["payload"];
        assert_eq!(
            terminal["request_id"],
            measurement["observation"]["request_id"]
        );
        let sent = measurement["harness_sent_ns"]
            .as_str()
            .unwrap()
            .parse::<u64>()
            .unwrap();
        let received = measurement["harness_received_ns"]
            .as_str()
            .unwrap()
            .parse::<u64>()
            .unwrap();
        assert_eq!(terminal["elapsed_ns"], (received - sent).to_string());
    }
}

#[test]
fn correct_rejection_releases_its_copy_without_a_file_and_flow_remains_usable() {
    let mut fx = Fixture::new(4);
    let rejected = fx.admit(&["Read rejected."]);
    fx.drive(true);
    assert!(fx.requests.is_empty());
    assert!(fx.flow.failure().is_none());
    assert!(fx.flow.ledger().barrier(&rejected).unwrap());
    assert_eq!(fx.budget.gauges(), (0, 0));
    let kept = fx.admit(&["Read later."]);
    fx.drive(false);
    assert_eq!(fx.requests.len(), 1);
    let s::StateFrame::File { filing, .. } = &fx.requests[0] else {
        panic!("file");
    };
    assert_eq!(filing.block_id, kept[0]);
    let records = fx.records();
    assert_eq!(
        records
            .iter()
            .filter(|record| record["kind"] == "state.flow"
                && record["payload"]["phase"] == "correctly_rejected")
            .count(),
        1
    );
    assert_eq!(records.iter().filter(|record|record["kind"]=="state.flow" && record["payload"]["phase"]=="filed").count(),1);
    let rejection = records
        .iter()
        .find(|record| record["payload"]["phase"] == "correctly_rejected")
        .unwrap();
    let rejected_sequence = rejected[0].sequence.0.to_string();
    let result = records
        .iter()
        .find(|record| {
            record["kind"] == "ste100.result"
                && record["payload"]["target"]["source"]["sequence"].as_str()
                    == Some(rejected_sequence.as_str())
        })
        .unwrap();
    assert_eq!(
        rejection["payload"]["request_id"],
        result["payload"]["request_id"]
    );
}

#[test]
fn whole_copy_backlog_resumes_with_its_original_intake_time() {
    let mut fx = Fixture::new(2);
    let first = fx.admit(&["Read first."]);
    let waiting = fx.admit(&["Read second.", "Read third."]);
    let intake = fx.time();
    assert!(fx.budget.waiting_copy().is_some());
    assert_eq!(fx.budget.gauges().0, 1);
    fx.until_file();
    fx.answer_file(0, false);
    for _ in 0..128 {
        fx.transport().unwrap();
        fx.advance().unwrap();
        if fx.budget.waiting_copy().is_none() {
            break;
        }
    }
    assert!(fx.budget.waiting_copy().is_none());
    assert!(fx.flow.ledger().barrier(&first).unwrap());
    let facts = fx.flow.ledger().facts().collect::<Vec<_>>();
    assert_eq!(facts.len(), 2);
    assert!(facts.iter().all(|fact| fact.intake_ns == intake));
    assert_eq!(
        facts.iter().map(|fact| fact.source).collect::<Vec<_>>(),
        waiting.iter().collect::<Vec<_>>()
    );
    fx.drive(false);
    assert_eq!(fx.requests.len(), 3);
    assert_eq!(fx.budget.gauges(), (0, 0));
    let records = fx.records();
    let transitions = records
        .iter()
        .filter(|record| record["kind"] == "state.flow")
        .filter_map(|record| record["payload"]["phase"].as_str())
        .filter(|phase| phase.starts_with("backlog_"))
        .collect::<Vec<_>>();
    assert_eq!(transitions, ["backlog_wait", "backlog_resumed"]);
}

#[test]
fn queued_backlog_notice_keeps_its_waiting_copy_after_an_earlier_terminal_releases_it() {
    let mut fx = Fixture::new(1);
    let first = fx.admit(&["Read first."]);
    fx.until_file();
    fx.answer_file(0, false);
    for _ in 0..128 {
        fx.transport().unwrap();
        fx.advance().unwrap();
        if fx
            .flow
            .ledger()
            .facts()
            .any(|fact| fact.state == LedgerState::FiledHeld)
        {
            break;
        }
    }
    assert!(
        fx.flow
            .ledger()
            .facts()
            .any(|fact| fact.state == LedgerState::FiledHeld)
    );
    assert!(!fx.flow.ledger().barrier(&first).unwrap());
    assert!(
        !fx.records()
            .iter()
            .any(|record| record["payload"]["phase"] == "filed")
    );
    // Filed is queued but has not released the only reservation. B enters the
    // waiting slot. Recording Filed resumes B before BacklogWait is authored.
    let waiting = fx.admit(&["Read second."]);
    assert!(fx.budget.waiting_copy().is_some());
    fx.drive(false);
    assert!(fx.flow.failure().is_none());
    assert!(fx.flow.ledger().barrier(&first).unwrap());
    assert!(fx.flow.ledger().barrier(&waiting).unwrap());
    assert_eq!(fx.budget.gauges(), (0, 0));
    assert_eq!(fx.requests.len(), 2);
    let records = fx.records();
    let wait = records
        .iter()
        .find(|record| record["payload"]["phase"] == "backlog_wait")
        .unwrap();
    let resumed = records
        .iter()
        .find(|record| record["payload"]["phase"] == "backlog_resumed")
        .unwrap();
    assert_eq!(wait["payload"]["backlog_waiting"], true);
    assert_eq!(wait["payload"]["retained_blocks"], "1");
    assert_eq!(wait["payload"]["pending_count"], "0");
    assert_eq!(wait["payload"]["filed_count"], "0");
    assert_eq!(
        wait["payload"]["waiting_copy"]["source_event"]["sequence"],
        waiting[0].sequence.0.to_string()
    );
    assert_eq!(resumed["payload"]["backlog_waiting"], false);
    assert!(resumed["payload"]["waiting_copy"].is_null());
    assert_eq!(resumed["payload"]["pending_count"], "1");
    let total_filed = records
        .iter()
        .filter(|record| record["kind"] == "state.flow")
        .map(|record| {
            record["payload"]["filed_count"]
                .as_str()
                .unwrap()
                .parse::<u64>()
                .unwrap()
        })
        .sum::<u64>();
    assert_eq!(
        total_filed, 2,
        "captured counters are charged to one interval each"
    );
}

#[test]
fn malformed_receipt_latches_first_failure_without_resend_or_later_disposal() {
    let mut fx = Fixture::new(4);
    let sources = fx.admit(&["Read first.", "Read later."]);
    fx.until_file();
    fx.answer_file(0, true);
    let failure = fx.transport().expect_err("wrong request identity");
    assert_eq!(fx.flow.failure(), Some(&failure));
    assert!(fx.budget.is_frozen());
    assert!(fx.barrier.recorder().tap_stopped());
    let gauges = fx.budget.gauges();
    assert_eq!(gauges.0, 2);
    assert_eq!(fx.flow.ledger().facts().count(), 2);
    for _ in 0..3 {
        assert_eq!(fx.advance().unwrap_err(), failure);
        assert_eq!(fx.transport().unwrap_err(), failure);
    }
    fx.collect_requests();
    assert_eq!(
        fx.requests.len(),
        1,
        "no retry or later filing after first failure"
    );
    assert_eq!(fx.budget.gauges(), gauges);
    assert!(fx.flow.ledger().barrier(&sources).is_err());
    let progress = &fx.flow.state().outstanding()[0];
    assert_eq!(
        progress.commit,
        super::super::channel::CommitEvidence::Unknown
    );
    assert!(
        !fx.records()
            .iter()
            .any(|record| record["payload"]["phase"] == "filed")
    );
}

#[test]
fn classifier_loss_with_a_held_acceptance_stops_before_any_file_request() {
    for close in [ScriptStep::Close, ScriptStep::ShutdownWrite] {
        let mut fx = Fixture::new(4);
        let sources = fx.admit(&["Read retained."]);
        for _ in 0..128 {
            fx.advance().unwrap();
            fx.answer_classifier(false);
            fx.transport().unwrap();
            if fx.flow.classifier().state() == ClassifierState::ResultHeld {
                break;
            }
        }
        let held = fx.flow.classifier().held().unwrap().clone();
        assert!(matches!(held.result, s::Ste100Result::Accepted { .. }));
        assert_eq!(held.target, s::Ste100Target::Source(sources[0].clone()));
        fx.classifier.push(close);
        assert!(fx.classifier.advance().unwrap());
        let failure = fx
            .transport()
            .expect_err("held answer cannot hide member loss");
        assert_eq!(failure.point, "classifier");
        assert_eq!(fx.flow.classifier().held(), Some(&held));
        assert!(fx.budget.is_frozen());
        assert_eq!(fx.budget.gauges().0, 1);
        assert!(fx.barrier.recorder().tap_stopped());
        assert_eq!(fx.advance().unwrap_err(), failure);
        fx.collect_requests();
        assert!(fx.requests.is_empty());
        assert!(!fx.records().iter().any(|record| {
            record["payload"]["phase"] == "filed" || record["payload"]["phase"] == "accepted"
        }));
    }
}

fn assert_recorded_component_failure(
    fx: &mut Fixture,
    source: &SourceBlockId,
    reply: &s::Ste100Reply,
    failure: &FlowFailure,
) {
    assert_eq!(failure.point, "classifier");
    assert_eq!(fx.flow.failure(), Some(failure));
    let s::Ste100Result::ComponentFailed { code, detail } = &reply.result else {
        panic!("component failure fixture");
    };
    let port_failure = fx.flow.classifier().failure().unwrap();
    assert_eq!(
        port_failure.kind,
        crate::flow::classifier::PortFailureKind::Component(*code)
    );
    assert_eq!(&port_failure.detail, detail);
    let records = fx.records();
    let results = records
        .iter()
        .filter(|record| record["kind"] == "ste100.result")
        .collect::<Vec<_>>();
    assert_eq!(results.len(), 1);
    assert_eq!(results[0]["payload"], serde_json::to_value(reply).unwrap());
    assert!(fx.budget.is_frozen());
    assert_eq!(fx.budget.gauges().0, 1);
    assert!(fx.barrier.recorder().tap_stopped());
    let facts = fx.flow.ledger().facts().collect::<Vec<_>>();
    assert_eq!(facts.len(), 1);
    assert_eq!(facts[0].source, source);
    assert_eq!(facts[0].state, LedgerState::AwaitingClassification);
    assert_eq!(
        fx.flow.classifier().outstanding().unwrap().request_id,
        reply.request_id
    );
    fx.collect_requests();
    assert!(fx.requests.is_empty());
    assert!(!records.iter().any(|record| {
        matches!(
            record["payload"]["phase"].as_str(),
            Some("accepted" | "file_started" | "filed" | "correctly_rejected")
        )
    }));
    let record_count = records.len();
    assert_eq!(fx.advance().unwrap_err(), *failure);
    assert_eq!(fx.transport().unwrap_err(), *failure);
    assert_eq!(fx.records().len(), record_count);
}

#[test]
fn complete_component_failure_is_recorded_before_halt_when_the_classifier_closes() {
    for code in [
        s::Ste100ComponentCode::Crashed,
        s::Ste100ComponentCode::NoResult,
        s::Ste100ComponentCode::InvalidResult,
        s::Ste100ComponentCode::TransportFailed,
    ] {
        for half_close in [false, true] {
            let mut fx = Fixture::new(4);
            let source = fx.admit(&["Read retained."]).remove(0);
            let request = fx.until_classifier_request();
            let mut reply = accepted(&request);
            reply.result = s::Ste100Result::ComponentFailed {
                code,
                detail: format!("fixture terminal result {code:?}"),
            };
            fx.classifier.push(ScriptStep::Reply(reply.clone()));
            assert!(fx.classifier.advance().unwrap());
            fx.classifier.push(if half_close {
                ScriptStep::ShutdownWrite
            } else {
                ScriptStep::Close
            });
            assert!(fx.classifier.advance().unwrap());
            fx.transport().unwrap();
            assert_eq!(fx.flow.classifier().held(), Some(&reply));
            let failure = (0..128)
                .find_map(|_| {
                    fx.transport().unwrap();
                    fx.advance().err()
                })
                .expect("recorded terminal reply must halt the flow");
            assert_recorded_component_failure(&mut fx, &source, &reply, &failure);
        }
    }
}

#[test]
fn trace_pressure_keeps_terminal_classifier_reply_until_its_exact_record_after_close() {
    let mut fx = Fixture::new(4);
    let source = fx.admit(&["Read retained."]).remove(0);
    let request = fx.until_classifier_request();
    let pending = fx.fill_trace_capacity();
    let mut reply = accepted(&request);
    reply.result = s::Ste100Result::ComponentFailed {
        code: s::Ste100ComponentCode::NoResult,
        detail: "fixture reported no_result before closing".into(),
    };
    fx.classifier.push(ScriptStep::Reply(reply.clone()));
    assert!(fx.classifier.advance().unwrap());
    fx.classifier.push(ScriptStep::Close);
    assert!(fx.classifier.advance().unwrap());
    fx.transport().unwrap();
    let charge = fx.budget.gauges();
    for _ in 0..4 {
        fx.transport().unwrap();
        assert_eq!(fx.advance().unwrap(), Advance::Pending);
        assert_eq!(fx.flow.classifier().held(), Some(&reply));
        assert!(fx.flow.failure().is_none());
        assert_eq!(fx.budget.gauges(), charge);
        assert_eq!(
            serde_json::to_string(fx.barrier.pending().unwrap()).unwrap(),
            pending
        );
        assert!(
            !fx.records()
                .iter()
                .any(|record| record["kind"] == "ste100.result")
        );
        fx.collect_requests();
        assert!(fx.requests.is_empty());
    }
    let reader = fx.start_drain();
    fx.resume_external_ticket();
    let failure = (0..128)
        .find_map(|_| {
            fx.transport().unwrap();
            fx.advance().err()
        })
        .expect("recorded terminal reply must halt after trace capacity returns");
    assert_recorded_component_failure(&mut fx, &source, &reply, &failure);
    drop(fx);
    reader.join().unwrap();
}

#[test]
fn trace_backpressure_holds_valid_completion_and_prevents_the_next_file() {
    let mut fx = Fixture::new(4);
    fx.admit(&["Read first.", "Read second."]);
    fx.until_file();
    let large = "x".repeat(32768);
    let mut pending = false;
    for _ in 0..ORDINARY_CAPACITY + 64 {
        let now = fx.time();
        let event = fx
            .author
            .prepare(
                &authored(
                    Kind::ModelMeasurement,
                    AuthoredSource::Runtime,
                    Payload::ModelMeasurement(
                        serde_json::value::to_raw_value(&serde_json::json!({"fixture":large}))
                            .unwrap(),
                    ),
                    true,
                ),
                now,
                1,
            )
            .unwrap();
        match fx.barrier.submit_with_clock(
            event,
            RecordPurpose::Observation,
            &mut fx.inbox,
            &mut fx.budget,
            &mut || Ok(now),
        ) {
            RecordProgress::Recorded { .. } => {}
            RecordProgress::Pending => {
                pending = true;
                break;
            }
            other => panic!("fill trace: {other:?}, failure: {:?}", fx.barrier.failure()),
        }
    }
    assert!(pending);
    let exact = serde_json::to_string(fx.barrier.pending().unwrap()).unwrap();
    fx.answer_file(0, false);
    fx.transport().unwrap();
    let held = fx.flow.state().held_measurement().unwrap().clone();
    let exact_elapsed = held.harness_received_ns.0 - held.harness_sent_ns.0;
    let charge = fx.budget.gauges();
    fx.elapse(10_000);
    for _ in 0..4 {
        assert_eq!(fx.advance().unwrap(), Advance::Pending);
        fx.transport().unwrap();
        fx.collect_requests();
        assert_eq!(fx.requests.len(), 1);
        assert_eq!(fx.budget.gauges(), charge);
        assert_eq!(
            serde_json::to_string(fx.barrier.pending().unwrap()).unwrap(),
            exact
        );
        assert!(fx.flow.state().held_measurement().is_some());
    }
    let reader = fx.start_drain();
    let deadline = Instant::now() + Duration::from_secs(8);
    loop {
        let now = fx.time();
        match fx
            .barrier
            .resume_with_clock(&mut fx.inbox, &mut fx.budget, &mut || Ok(now))
        {
            RecordProgress::Recorded { .. } => break,
            RecordProgress::Pending => {
                assert!(Instant::now() < deadline);
                std::thread::yield_now();
            }
            other => panic!("pending author: {other:?}"),
        }
    }
    fx.drive(false);
    assert_eq!(fx.requests.len(), 2);
    assert_eq!(fx.budget.gauges(), (0, 0));
    let records = fx.records();
    let request_id = held.observation.request_id.0.to_string();
    let terminal = records
        .iter()
        .find(|record| {
            record["kind"] == "state.flow"
                && record["payload"]["phase"] == "filed"
                && record["payload"]["request_id"].as_str() == Some(request_id.as_str())
        })
        .unwrap();
    assert_eq!(terminal["payload"]["elapsed_ns"], exact_elapsed.to_string());
    assert!(fx.time() - held.harness_sent_ns.0 > exact_elapsed + 10_000);
    assert_eq!(
        records
            .iter()
            .filter(|record| record["kind"] == "state.store.measurement")
            .count(),
        3,
        "one open plus two file observations"
    );
    drop(fx);
    reader.join().unwrap();
}

#[test]
fn ignored_external_author_failure_stops_before_an_unsent_file_reaches_state() {
    for transport_first in [false, true] {
        let mut fx = Fixture::new(4);
        fx.admit(&["Read retained."]);
        for _ in 0..128 {
            fx.advance().unwrap();
            if !fx.flow.state().outstanding().is_empty() {
                break;
            }
            fx.answer_classifier(false);
            fx.transport().unwrap();
        }
        let outstanding = fx.flow.state().outstanding();
        assert_eq!(outstanding.len(), 1);
        assert_eq!(outstanding[0].bytes_sent, 0);
        fx.collect_requests();
        assert!(fx.requests.is_empty());
        let now = fx.time();
        let mut event = fx
            .author
            .prepare(
                &authored(
                    Kind::ModelMeasurement,
                    AuthoredSource::Runtime,
                    Payload::ModelMeasurement(
                        serde_json::value::to_raw_value(&serde_json::json!({"fixture":true}))
                            .unwrap(),
                    ),
                    false,
                ),
                now,
                1,
            )
            .unwrap();
        event.envelope.run = RunRef("wrong-run".into());
        let before = fx.barrier.recorder().structure().len();
        // The surrounding caller loses this return. The shared owner still
        // exposes the first failure before either coordinator entry can act.
        let _ = fx.barrier.submit_with_clock(
            event,
            RecordPurpose::Observation,
            &mut fx.inbox,
            &mut fx.budget,
            &mut || Ok(now),
        );
        assert!(fx.barrier.failure().is_some());
        let failure = if transport_first {
            fx.transport().unwrap_err()
        } else {
            fx.advance().unwrap_err()
        };
        assert_eq!(failure.point, "author");
        assert_eq!(fx.transport().unwrap_err(), failure);
        assert_eq!(fx.advance().unwrap_err(), failure);
        assert!(fx.budget.is_frozen());
        assert!(fx.barrier.recorder().tap_stopped());
        assert_eq!(fx.budget.gauges().0, 1);
        assert_eq!(fx.flow.ledger().facts().count(), 1);
        assert_eq!(fx.flow.state().outstanding()[0].bytes_sent, 0);
        fx.collect_requests();
        assert!(fx.requests.is_empty());
        assert_eq!(fx.barrier.recorder().structure().len(), before);
    }
}

impl Fixture {
    fn begin_control(&mut self, request: s::StateControl) -> OperationStart<s::StateControl> {
        match self
            .flow
            .begin_control(
                &mut FlowRecording {
                    barrier: &mut self.barrier,
                    inbox: &mut self.inbox,
                    budget: &mut self.budget,
                    author: &self.author,
                },
                &mut self.clock,
                ControlOperation::try_from((request, Some(TurnRef("control-turn".into()))))
                    .unwrap(),
            )
            .unwrap()
        {
            OperationStart::Started { request_id } => OperationStart::Started { request_id },
            OperationStart::Busy(request) => {
                let (request, turn) = request.into_parts();
                assert_eq!(turn, Some(TurnRef("control-turn".into())));
                OperationStart::Busy(request)
            }
        }
    }

    fn take_operation(&mut self) -> Result<Option<OperationCompletion>, FlowFailure> {
        self.flow.take_operation_completion(&mut FlowRecording {
            barrier: &mut self.barrier,
            inbox: &mut self.inbox,
            budget: &mut self.budget,
            author: &self.author,
        })
    }

    fn begin_check(&mut self, operation: CheckOperation) -> OperationStart<CheckOperation> {
        self.flow
            .begin_check(
                &mut FlowRecording {
                    barrier: &mut self.barrier,
                    inbox: &mut self.inbox,
                    budget: &mut self.budget,
                    author: &self.author,
                },
                operation,
            )
            .unwrap()
    }

    fn record_unselected(&mut self, event: AuthoredEvent) -> u64 {
        let now = self.time();
        let event = self.author.prepare(&event, now, 1).unwrap();
        let RecordProgress::Recorded { sequence, .. } = self.barrier.submit_with_clock(
            event,
            RecordPurpose::Observation,
            &mut self.inbox,
            &mut self.budget,
            &mut || Ok(now),
        ) else {
            panic!("unselected event must record");
        };
        assert_eq!(self.budget.gauges(), (0, 0));
        sequence.0
    }

    fn answer_control(&mut self, frame: s::StateControlAnswer) {
        self.transport().unwrap();
        self.collect_requests();
        let request = self.requests.last().expect("control reached socket");
        assert!(matches!(request, s::StateFrame::Control { .. }));
        let observation = operation(request, self.operation, self.clock.now().unwrap() + 1);
        self.operation += 1;
        let answer = s::StateFrame::ControlAnswer {
            common: request.common().clone(),
            frame,
            observation,
        };
        self.state_peer
            .write_all(&s::encode_state_line(&answer, s::FrameDirection::Response).unwrap())
            .unwrap();
        self.elapse(3);
        self.transport().unwrap();
    }

    fn reply_check(&mut self, result: s::Ste100Result) -> s::Ste100Reply {
        let request = self.classifier.submitted().last().unwrap().clone();
        let reply = s::Ste100Reply {
            request_id: request.request_id,
            target: request.target,
            scheme_id: request.scheme_id,
            result,
        };
        self.classifier.push(ScriptStep::Reply(reply.clone()));
        assert!(self.classifier.advance().unwrap());
        self.transport().unwrap();
        reply
    }
}

#[test]
fn control_completion_is_recorded_once_without_a_ledger_obligation() {
    let mut fx = Fixture::new(4);
    assert!(matches!(
        fx.begin_control(s::StateControl::Shape {}),
        OperationStart::Started { .. }
    ));
    assert_eq!(
        fx.begin_control(s::StateControl::Grants {}),
        OperationStart::Busy(s::StateControl::Grants {})
    );
    fx.answer_control(s::StateControlAnswer::Shape(vec![s::ShapeRow {
        run: "prior".into(),
        kind: "message.user".into(),
        count: 17.into(),
    }]));
    assert!(fx.take_operation().unwrap().is_none());
    assert!(fx.flow.state().held_measurement().is_some());
    fx.advance().unwrap();
    let OperationCompletion::State {
        request,
        turn,
        completion,
    } = fx.take_operation().unwrap().unwrap()
    else {
        panic!("typed state completion");
    };
    assert_eq!(request, s::StateControl::Shape {});
    assert_eq!(turn, Some(TurnRef("control-turn".into())));
    assert_eq!(
        fx.records()
            .iter()
            .rev()
            .find(|record| record["kind"] == "state.store.measurement")
            .unwrap()["turn"],
        "control-turn",
    );
    let s::StateFrame::ControlAnswer {
        frame: s::StateControlAnswer::Shape(rows),
        ..
    } = completion.answer
    else {
        panic!("exact shape answer");
    };
    assert_eq!(rows[0].count.0, 17);
    assert!(!completion.discard_capture_result);
    assert!(fx.take_operation().unwrap().is_none());
    assert!(fx.flow.ledger().facts().next().is_none());
    assert_eq!(fx.budget.gauges(), (0, 0));
    assert_eq!(
        fx.records()
            .iter()
            .filter(|r| r["kind"] == "state.store.measurement")
            .count(),
        2
    );
    assert!(
        !fx.records()
            .iter()
            .any(|r| r["payload"]["phase"] == "filed")
    );
}

#[test]
fn correct_read_refusal_is_an_operation_result_and_next_control_can_run() {
    let mut fx = Fixture::new(4);
    let request = s::StateControl::Read {
        selector: s::ReadSelector::Conversation {
            last_turns: 1.into(),
        },
    };
    assert!(matches!(
        fx.begin_control(request.clone()),
        OperationStart::Started { .. }
    ));
    fx.answer_control(s::StateControlAnswer::ControlRefused {
        code: s::ControlRefusalCode::Read(s::ReadRefusalCode::ResultUnavailable),
        detail: "test-only snapshot refused".into(),
    });
    fx.advance().unwrap();
    let OperationCompletion::State {
        request: actual,
        completion,
        ..
    } = fx.take_operation().unwrap().unwrap()
    else {
        panic!("operation refusal");
    };
    assert_eq!(actual, request);
    assert!(matches!(completion.answer, s::StateFrame::ControlAnswer {
        frame: s::StateControlAnswer::ControlRefused {
            code: s::ControlRefusalCode::Read(s::ReadRefusalCode::ResultUnavailable), ref detail,
        }, ..
    } if detail == "test-only snapshot refused"));
    assert!(fx.flow.failure().is_none());
    assert!(matches!(
        fx.begin_control(s::StateControl::Held {}),
        OperationStart::Started { .. }
    ));
}

#[test]
fn template_and_unselected_model_results_never_enter_the_filing_ledger() {
    let mut fx = Fixture::new(4);
    let template = s::TemplateSpec {
        template_id: "fixture-read".into(),
        kind: "loop.instruction".into(),
        parts: vec![s::TemplatePart::Literal("Read file.".into())],
    };
    let operation = CheckOperation::Template {
        template_id: template.template_id.clone(),
        text: serde_json::to_string(&template).unwrap(),
    };
    assert!(matches!(
        fx.begin_check(operation.clone()),
        OperationStart::Started { .. }
    ));
    assert_eq!(
        fx.begin_check(operation.clone()),
        OperationStart::Busy(operation.clone())
    );
    let reply = fx.reply_check(s::Ste100Result::Accepted { bindings: vec![] });
    assert!(fx.take_operation().unwrap().is_none());
    fx.advance().unwrap();
    let OperationCompletion::Check(recorded) = fx.take_operation().unwrap().unwrap() else {
        panic!("template completion");
    };
    assert_eq!(recorded.operation(), &operation);
    assert_eq!(recorded.reply(), &reply);

    let sequence = fx.record_unselected(authored(
        Kind::ModelOutput,
        AuthoredSource::Model,
        Payload::ModelOutput(weaver_trace::ModelOutput {
            emission: "not conforming".into(),
            finish: weaver_trace::Finish::Completed,
            resident: 10,
            capacity: 100,
        }),
        true,
    ));
    let source = s::SourceBlockId {
        run: "run".into(),
        sequence: sequence.into(),
        block: 0.into(),
    };
    let operation = CheckOperation::UnselectedModel {
        source,
        text: "not conforming".into(),
        turn: TurnRef("turn".into()),
    };
    assert!(matches!(
        fx.begin_check(operation.clone()),
        OperationStart::Started { .. }
    ));
    let reply = fx.reply_check(s::Ste100Result::NotSte100 {
        code: s::Ste100DefectCode::NonSte100,
        detail: "repair required".into(),
        spans: vec![],
    });
    fx.advance().unwrap();
    let OperationCompletion::Check(recorded) = fx.take_operation().unwrap().unwrap() else {
        panic!("model repair disposition");
    };
    assert_eq!(recorded.operation(), &operation);
    assert_eq!(recorded.reply(), &reply);
    assert!(fx.flow.failure().is_none());
    assert_eq!(
        fx.records()
            .iter()
            .rev()
            .find(|record| record["kind"] == "ste100.result")
            .unwrap()["turn"],
        "turn",
    );
    assert!(fx.flow.ledger().facts().next().is_none());
    assert_eq!(fx.budget.gauges(), (0, 0));
    fx.collect_requests();
    assert!(fx.requests.is_empty());
    assert_eq!(
        fx.records()
            .iter()
            .filter(|r| r["kind"] == "ste100.result")
            .count(),
        2
    );
}

fn template_operation() -> CheckOperation {
    let template = s::TemplateSpec {
        template_id: "fixture-operation".into(),
        kind: "loop.instruction".into(),
        parts: vec![s::TemplatePart::Literal("Read file.".into())],
    };
    CheckOperation::Template {
        template_id: template.template_id.clone(),
        text: serde_json::to_string(&template).unwrap(),
    }
}

#[test]
fn nonfiling_completion_pressure_holds_both_native_slots_without_reauthoring() {
    let mut fx = Fixture::new(4);
    fx.begin_control(s::StateControl::Held {});
    fx.begin_check(template_operation());
    fx.answer_control(s::StateControlAnswer::Held { count: 23.into() });
    let reply = fx.reply_check(s::Ste100Result::Accepted { bindings: vec![] });
    let pending = fx.fill_trace_capacity();
    for _ in 0..3 {
        assert_eq!(fx.advance().unwrap(), Advance::Pending);
        assert!(fx.take_operation().unwrap().is_none());
        assert!(fx.flow.state().held_measurement().is_some());
        assert_eq!(fx.flow.classifier().held(), Some(&reply));
        assert_eq!(
            serde_json::to_string(fx.barrier.pending().unwrap()).unwrap(),
            pending
        );
    }
    let reader = fx.start_drain();
    fx.resume_external_ticket();
    for _ in 0..128 {
        fx.advance().unwrap();
        if fx.flow.operation_completion().is_some() {
            break;
        }
    }
    assert!(matches!(
        fx.flow.operation_completion(),
        Some(OperationCompletion::State { .. })
    ));
    let count = fx
        .records()
        .iter()
        .filter(|r| r["kind"] == "state.store.measurement")
        .count();
    for _ in 0..3 {
        assert_eq!(fx.advance().unwrap(), Advance::Pending);
    }
    assert_eq!(
        fx.records()
            .iter()
            .filter(|r| r["kind"] == "state.store.measurement")
            .count(),
        count
    );
    assert_eq!(fx.flow.classifier().held(), Some(&reply));
    assert!(matches!(
        fx.take_operation().unwrap(),
        Some(OperationCompletion::State { .. })
    ));
    for _ in 0..128 {
        fx.advance().unwrap();
        if fx.flow.operation_completion().is_some() {
            break;
        }
    }
    assert!(matches!(
        fx.take_operation().unwrap(),
        Some(OperationCompletion::Check(_))
    ));
    assert_eq!(
        fx.records()
            .iter()
            .filter(|r| r["kind"] == "ste100.result")
            .count(),
        1
    );
    assert_eq!(fx.budget.gauges(), (0, 0));
    drop(fx);
    reader.join().unwrap();
}

#[test]
fn one_transport_choice_does_not_progress_the_other_ready_owner() {
    let mut fx = Fixture::new(4);
    fx.begin_control(s::StateControl::Held {});
    fx.begin_check(template_operation());
    let request = fx.classifier.submitted().last().unwrap().clone();
    fx.classifier.push(ScriptStep::Reply(s::Ste100Reply {
        request_id: request.request_id,
        target: request.target,
        scheme_id: request.scheme_id,
        result: s::Ste100Result::Accepted { bindings: vec![] },
    }));
    assert!(fx.classifier.advance().unwrap());
    fx.flow
        .transport_one(
            &mut FlowRecording {
                barrier: &mut fx.barrier,
                inbox: &mut fx.inbox,
                budget: &mut fx.budget,
                author: &fx.author,
            },
            &mut fx.clock,
            TransportSource::State,
        )
        .unwrap();
    assert_eq!(fx.flow.classifier().state(), ClassifierState::InFlight);
    fx.collect_requests();
    assert_eq!(fx.requests.len(), 1);
    fx.flow
        .transport_one(
            &mut FlowRecording {
                barrier: &mut fx.barrier,
                inbox: &mut fx.inbox,
                budget: &mut fx.budget,
                author: &fx.author,
            },
            &mut fx.clock,
            TransportSource::Classifier,
        )
        .unwrap();
    assert_eq!(fx.flow.classifier().state(), ClassifierState::ResultHeld);
    assert!(fx.take_operation().unwrap().is_none());
}

#[test]
fn nonfiling_component_failure_records_exact_result_and_keeps_its_owner() {
    let mut fx = Fixture::new(4);
    let operation = template_operation();
    fx.begin_check(operation.clone());
    let reply = fx.reply_check(s::Ste100Result::ComponentFailed {
        code: s::Ste100ComponentCode::NoResult,
        detail: "test-only template component failure".into(),
    });
    fx.classifier.push(ScriptStep::Close);
    assert!(fx.classifier.advance().unwrap());
    fx.transport().unwrap();
    assert!(fx.advance().is_err());
    assert!(fx.take_operation().is_err());
    assert_eq!(fx.flow.pending_check(), Some(&operation));
    assert!(fx.flow.ledger().facts().next().is_none());
    assert!(fx.budget.is_frozen());
    let actual = fx
        .records()
        .into_iter()
        .find(|r| r["kind"] == "ste100.result")
        .unwrap();
    assert_eq!(actual["payload"], serde_json::to_value(reply).unwrap());
}

#[test]
fn returned_second_author_keeps_original_six_members_for_failure_reporting() {
    let mut fx = Fixture::new(4);
    let first = fx.fill_trace_capacity();
    let reply = s::Ste100Reply {
        request_id: 99.into(),
        target: s::Ste100Target::Input {
            input_id: 2.into(),
            block: 0.into(),
        },
        scheme_id: "fixture-read/1".into(),
        result: s::Ste100Result::NotSte100 {
            code: s::Ste100DefectCode::NonSte100,
            detail: "test-only refused second author".into(),
            spans: vec![],
        },
    };
    let error = fx
        .flow
        .author(
            &mut FlowRecording {
                barrier: &mut fx.barrier,
                inbox: &mut fx.inbox,
                budget: &mut fx.budget,
                author: &fx.author,
            },
            &mut fx.clock,
            1,
            Kind::Ste100Result,
            None,
            Payload::Ste100Result(Box::new(super::super::mirror::ste100(&reply))),
            RecordPurpose::Classifier(reply.clone()),
            PendingRecord::Classifier,
        )
        .unwrap_err();
    assert_eq!(error.point, "author");
    assert_eq!(
        serde_json::to_string(fx.barrier.pending().unwrap()).unwrap(),
        first
    );
    let Some(RefusedAuthor::Authored {
        event,
        purpose: RecordPurpose::Classifier(held_reply),
    }) = fx.flow.refused_author()
    else {
        panic!("returned author must remain owned");
    };
    assert_eq!(event.kind, Kind::Ste100Result);
    assert_eq!(event.subsystem, Subsystem::Harness);
    assert!(event.turn.is_none());
    assert!(matches!(event.source, AuthoredSource::Runtime));
    assert!(event.causal_parent.is_none());
    assert_eq!(*held_reply, reply);
    assert_eq!(
        serde_json::to_value(event.payload.as_ref().unwrap()).unwrap(),
        serde_json::to_value(reply).unwrap()
    );
    let reader = fx.start_drain();
    drop(fx);
    reader.join().unwrap();
}

#[test]
fn capture_controls_cannot_enter_the_unpaired_operation_route() {
    for control in [
        s::StateControl::Snapshot {
            capture_id: 7.into(),
        },
        s::StateControl::Acknowledge {
            capture_id: 7.into(),
            dump_sha256: s::Digest::new("a".repeat(64)).unwrap(),
        },
        s::StateControl::Abort {
            capture_id: 7.into(),
        },
        s::StateControl::ReleaseCapture {
            capture_id: 7.into(),
        },
    ] {
        let turn = Some(TurnRef("capture-turn".into()));
        assert_eq!(
            ControlOperation::try_from((control.clone(), turn.clone())),
            Err((control, turn))
        );
    }
}

#[test]
fn nonfiling_wait_stays_pending_without_fabricated_flow_work() {
    let mut fx = Fixture::new(4);
    fx.begin_check(template_operation());
    fx.elapse(2_000_000);
    assert_eq!(fx.advance().unwrap(), Advance::Pending);
    assert!(fx.flow.ledger().facts().next().is_none());
    assert_eq!(fx.budget.gauges(), (0, 0));
    assert!(!fx.records().iter().any(|r| r["kind"] == "state.flow"));
}

#[test]
fn busy_control_returns_its_entire_request_and_distinct_turn() {
    let mut fx = Fixture::new(4);
    fx.begin_control(s::StateControl::Held {});
    let request = ControlOperation::try_from((
        s::StateControl::Grants {},
        Some(TurnRef("second-turn".into())),
    ))
    .unwrap();
    let result = fx
        .flow
        .begin_control(
            &mut FlowRecording {
                barrier: &mut fx.barrier,
                inbox: &mut fx.inbox,
                budget: &mut fx.budget,
                author: &fx.author,
            },
            &mut fx.clock,
            request,
        )
        .unwrap();
    let OperationStart::Busy(returned) = result else {
        panic!("second control is busy");
    };
    assert_eq!(
        returned.into_parts(),
        (
            s::StateControl::Grants {},
            Some(TurnRef("second-turn".into()))
        )
    );
    assert_eq!(
        fx.flow.pending_control(),
        Some((
            &s::StateControl::Held {},
            Some(&TurnRef("control-turn".into()))
        ))
    );
}

#[test]
fn selected_copy_cannot_be_relabelled_as_an_unselected_model_check() {
    for installed in [false, true] {
        let mut fx = Fixture::new(4);
        let source = fx.admit(&["Read file."])[0].clone();
        if installed {
            fx.flow
                .drain(
                    &mut FlowRecording {
                        barrier: &mut fx.barrier,
                        inbox: &mut fx.inbox,
                        budget: &mut fx.budget,
                        author: &fx.author,
                    },
                    &mut fx.clock,
                )
                .unwrap();
        }
        let result = fx.flow.begin_check(
            &mut FlowRecording {
                barrier: &mut fx.barrier,
                inbox: &mut fx.inbox,
                budget: &mut fx.budget,
                author: &fx.author,
            },
            CheckOperation::UnselectedModel {
                source,
                text: "Read file.".into(),
                turn: TurnRef("turn".into()),
            },
        );
        assert!(result.is_err());
        assert!(fx.classifier.submitted().is_empty());
        assert_eq!(fx.budget.gauges().0, 1);
        assert_eq!(fx.flow.ledger().facts().count(), usize::from(installed));
        assert_eq!(fx.inbox.queued().count(), usize::from(!installed));
    }
}

#[test]
fn interrupted_release_waits_for_a_later_external_author_without_recording_twice() {
    let mut fx = Fixture::new(4);
    fx.begin_control(s::StateControl::Held {});
    fx.answer_control(s::StateControlAnswer::Held { count: 31.into() });
    let measurement = fx.flow.state().held_measurement().unwrap().clone();
    let event = authored(
        Kind::StateStoreMeasurement,
        AuthoredSource::Runtime,
        Payload::StateStoreMeasurement(Box::new(super::super::mirror::measurement(&measurement))),
        true,
    );
    let RecordProgress::Recorded {
        proof: Some(RecordProof::State(proof)),
        ..
    } = fx.barrier.submit_authored_with_clock(
        event,
        RecordPurpose::State(measurement.clone()),
        &fx.author,
        1,
        &mut fx.inbox,
        &mut fx.budget,
        &mut || fx.clock.now(),
    )
    else {
        panic!("the exact state measurement must actually record");
    };
    // Only the final nonblocking peek is interrupted. The real proof stays in
    // StateChannel. Establish the matching coordinator continuation exactly as
    // recorded() does, without inventing a receipt or a successful release.
    fx.flow.state_release_timing = Some((
        measurement.harness_received_ns.0,
        measurement.harness_sent_ns.0,
    ));
    let result = fx
        .flow
        .state
        .as_mut()
        .unwrap()
        .release_interrupted_for_test(proof)
        .unwrap();
    assert!(result.is_none());
    assert_eq!(
        fx.flow
            .finish_state_release(
                &mut FlowRecording {
                    barrier: &mut fx.barrier,
                    inbox: &mut fx.inbox,
                    budget: &mut fx.budget,
                    author: &fx.author
                },
                &mut fx.clock,
                result,
            )
            .unwrap(),
        Advance::Pending
    );
    let first = fx.fill_trace_capacity();
    for _ in 0..3 {
        assert_eq!(fx.advance().unwrap(), Advance::Pending);
        assert!(fx.take_operation().unwrap().is_none());
        assert_eq!(fx.flow.state().held_measurement(), Some(&measurement));
        assert_eq!(
            serde_json::to_string(fx.barrier.pending().unwrap()).unwrap(),
            first
        );
    }
    let reader = fx.start_drain();
    fx.resume_external_ticket();
    assert_eq!(fx.advance().unwrap(), Advance::Progress);
    assert!(matches!(
        fx.take_operation().unwrap(),
        Some(OperationCompletion::State { .. })
    ));
    assert!(fx.flow.state().held_measurement().is_none());
    assert_eq!(
        fx.records()
            .iter()
            .filter(|r| r["kind"] == "state.store.measurement")
            .count(),
        2
    );
    assert!(fx.take_operation().unwrap().is_none());
    drop(fx);
    reader.join().unwrap();
}

struct BootFixture<P = ScriptedPort> {
    flow: FlowCoordinator<P>,
    peer: UnixStream,
    barrier: RecordBarrier,
    inbox: FlowInbox,
    budget: RetentionBudget,
    author: ServingAuthor,
    clock: FakePumpIo,
    reader: Option<UnixStream>,
}

impl BootFixture<ScriptedPort> {
    fn new() -> Self {
        let seed: s::SeedRequest = s::decode_strict(include_str!("fixtures/seed.json")).unwrap();
        Self::with_scheme(s::SchemeDeclaration {
            artifact_sha256: s::Digest::new(seed.scheme_id.strip_prefix("ste100:").unwrap())
                .unwrap(),
            scheme_id: seed.scheme_id,
            verbs: vec!["read".into()],
        })
    }
}

impl<P: Ste100Port> BootFixture<P> {
    fn with_scheme(scheme: s::SchemeDeclaration) -> Self {
        let (sink, reader) = UnixStream::pair().unwrap();
        sink.set_write_timeout(Some(Duration::from_secs(10)))
            .unwrap();
        reader
            .set_read_timeout(Some(Duration::from_secs(10)))
            .unwrap();
        let recorder = PreflightRecorder::receive(
            sink.into(),
            RunRef("run".into()),
            SessionRef("session".into()),
        )
        .unwrap();
        let (socket, peer) = UnixStream::pair().unwrap();
        peer.set_read_timeout(Some(Duration::from_secs(3))).unwrap();
        peer.set_write_timeout(Some(Duration::from_secs(3)))
            .unwrap();
        Self {
            flow: FlowCoordinator::preflight(
                StateChannel::new(socket, "session".into(), "run".into()).unwrap(),
                scheme,
                s::MeasuredInterval {
                    elapsed_ns: 1000000.into(),
                    measurement_ref: "test-only cadence".into(),
                },
                2,
            )
            .unwrap(),
            peer,
            barrier: RecordBarrier::preflight(recorder, "run".into()),
            inbox: FlowInbox::new(),
            budget: RetentionBudget::new(s::FlowBacklog {
                max_pending_blocks: 4.into(),
                max_pending_bytes: 100000.into(),
                lag_alarm_turns: 2.into(),
                measurement_ref: "test-only capacity".into(),
            })
            .unwrap(),
            author: ServingAuthor::new("session".into(), "run".into(), 0, 100).unwrap(),
            clock: FakePumpIo::new(100),
            reader: Some(reader),
        }
    }

    fn begin_and_answer_open(&mut self) {
        self.flow
            .begin_open(
                &mut FlowRecording {
                    barrier: &mut self.barrier,
                    inbox: &mut self.inbox,
                    budget: &mut self.budget,
                    author: &self.author,
                },
                &mut self.clock,
                serde_json::from_value(
                    serde_json::to_value(fixture::load_record().state_start).unwrap(),
                )
                .unwrap(),
                s::ReadLimits {
                    max_result_bytes: 100000.into(),
                    measurement_ref: "test-only reads".into(),
                },
            )
            .unwrap();
        self.clock.advance_to(110).unwrap();
        self.flow
            .transport_one(
                &mut FlowRecording {
                    barrier: &mut self.barrier,
                    inbox: &mut self.inbox,
                    budget: &mut self.budget,
                    author: &self.author,
                },
                &mut self.clock,
                TransportSource::State,
            )
            .unwrap();
        let mut bytes = Vec::new();
        BufReader::new(&mut self.peer)
            .read_until(b'\n', &mut bytes)
            .unwrap();
        let request = s::decode_state_line(&bytes, s::FrameDirection::Request).unwrap();
        let observation = operation(&request, 1, 111);
        let s::StateFrame::Open {
            common,
            schema_id,
            scheme_id,
            start,
            read_limits,
        } = request
        else {
            panic!("open");
        };
        let answer = s::StateFrame::Opened {
            common,
            schema_id,
            scheme_id,
            start,
            read_limits,
            observation,
            member_observation: s::MemberObservationIdentity {
                instrument_version: "state-observation/2".into(),
                pid: 7,
                start_ticks: Some(10.into()),
                ticks_per_second: Some(100.into()),
                unavailable_reason: None,
            },
        };
        self.peer
            .write_all(&s::encode_state_line(&answer, s::FrameDirection::Response).unwrap())
            .unwrap();
        self.clock.advance_to(120).unwrap();
        self.flow
            .transport_one(
                &mut FlowRecording {
                    barrier: &mut self.barrier,
                    inbox: &mut self.inbox,
                    budget: &mut self.budget,
                    author: &self.author,
                },
                &mut self.clock,
                TransportSource::State,
            )
            .unwrap();
    }

    fn load_only(&mut self) {
        let progress = self.barrier.submit_authored_with_clock(
            authored(
                Kind::Load,
                AuthoredSource::Runtime,
                Payload::Elections(Box::new(fixture::load_record())),
                false,
            ),
            RecordPurpose::Content,
            &self.author,
            100,
            &mut self.inbox,
            &mut self.budget,
            &mut || Ok(121),
        );
        assert!(
            matches!(progress, RecordProgress::Recorded { .. }),
            "load: {progress:?}, {:?}",
            self.barrier.failure()
        );
    }

    fn load_and_activate(&mut self) {
        self.load_only();
        self.barrier
            .activate(validate_partition(&fixture::partition()).unwrap())
            .unwrap();
        self.flow
            .activate_after_load(&mut FlowRecording {
                barrier: &mut self.barrier,
                inbox: &mut self.inbox,
                budget: &mut self.budget,
                author: &self.author,
            })
            .unwrap();
        self.clock.advance_to(122).unwrap();
    }

    fn advance(&mut self) -> Result<Advance, FlowFailure> {
        self.flow.advance(
            &mut FlowRecording {
                barrier: &mut self.barrier,
                inbox: &mut self.inbox,
                budget: &mut self.budget,
                author: &self.author,
            },
            &mut self.clock,
            100,
        )
    }
}

#[test]
fn preflight_open_is_owned_before_load_and_released_only_after_its_real_record() {
    let mut fx = BootFixture::new();
    let state_fd = std::os::fd::AsRawFd::as_raw_fd(&fx.flow.state().fd());
    assert_eq!(fx.flow.phase(), CoordinatorPhase::Preflight);
    assert!(fx.flow.standing_classifier().is_none());
    assert!(fx.flow.observations.is_none());
    assert_eq!(fx.advance().unwrap(), Advance::Pending);
    fx.begin_and_answer_open();
    assert_eq!(fx.flow.opened_identity().unwrap().pid, 7);
    assert!(fx.flow.state().held_measurement().is_some());
    assert!(!fx.flow.state().can_begin_filing());
    assert!(!fx.barrier.is_loaded());
    assert_eq!(fx.advance().unwrap(), Advance::Pending);
    assert!(fx.flow.operation_completion().is_none());
    assert!(fx.flow.ledger().facts().next().is_none());
    fx.load_and_activate();
    assert_eq!(fx.flow.phase(), CoordinatorPhase::Opening);
    assert!(fx.flow.observations.is_none());
    assert_eq!(fx.advance().unwrap(), Advance::Progress);
    assert_eq!(fx.flow.phase(), CoordinatorPhase::Active);
    assert!(fx.flow.observations.is_some());
    assert!(fx.flow.state().can_begin_filing());
    assert_eq!(
        std::os::fd::AsRawFd::as_raw_fd(&fx.flow.state().fd()),
        state_fd
    );
    assert!(matches!(
        fx.flow
            .take_operation_completion(&mut FlowRecording {
                barrier: &mut fx.barrier,
                inbox: &mut fx.inbox,
                budget: &mut fx.budget,
                author: &fx.author,
            })
            .unwrap(),
        Some(OperationCompletion::Opened(_))
    ));
    assert!(fx.flow.ledger().facts().next().is_none());
    assert_eq!(fx.budget.gauges(), (0, 0));
    assert!(fx.flow.standing_classifier().is_none());
    let mut reader = BufReader::new(fx.reader.take().unwrap());
    let mut kinds = Vec::new();
    for _ in 0..2 {
        let mut line = String::new();
        reader.read_line(&mut line).unwrap();
        kinds.push(serde_json::from_str::<serde_json::Value>(&line).unwrap()["kind"].clone());
    }
    assert_eq!(kinds, vec!["load", "state.store.measurement"]);
}

#[test]
fn open_cannot_activate_without_recorded_load_or_recreate_the_state_owner() {
    let mut fx = BootFixture::new();
    fx.begin_and_answer_open();
    let error = fx
        .flow
        .activate_after_load(&mut FlowRecording {
            barrier: &mut fx.barrier,
            inbox: &mut fx.inbox,
            budget: &mut fx.budget,
            author: &fx.author,
        })
        .unwrap_err();
    assert_eq!(error.point, "open");
    assert!(fx.flow.state().held_measurement().is_some());
    assert!(fx.flow.observations.is_none());
    assert!(fx.budget.is_frozen());
    assert!(fx.flow.operation_completion().is_none());
}

#[test]
fn clean_retirement_removes_ports_once_and_retains_the_empty_coordinator() {
    let mut fx = Fixture::new(4);
    fx.flow
        .begin_releasing(&mut FlowRecording {
            barrier: &mut fx.barrier,
            inbox: &mut fx.inbox,
            budget: &mut fx.budget,
            author: &fx.author,
        })
        .unwrap();
    assert_eq!(
        fx.flow
            .retire_state(
                &mut FlowRecording {
                    barrier: &mut fx.barrier,
                    inbox: &mut fx.inbox,
                    budget: &mut fx.budget,
                    author: &fx.author,
                },
                &mut fx.clock
            )
            .unwrap(),
        Advance::Progress
    );
    assert!(fx.flow.standing_state().is_none());
    assert_eq!(
        fx.flow
            .retire_classifier(&mut FlowRecording {
                barrier: &mut fx.barrier,
                inbox: &mut fx.inbox,
                budget: &mut fx.budget,
                author: &fx.author,
            })
            .unwrap(),
        Advance::Progress
    );
    assert!(fx.flow.standing_classifier().is_none());
    assert!(fx.flow.ledger().facts().next().is_none());
    fx.flow
        .close(&mut FlowRecording {
            barrier: &mut fx.barrier,
            inbox: &mut fx.inbox,
            budget: &mut fx.budget,
            author: &fx.author,
        })
        .unwrap();
    assert_eq!(fx.flow.phase(), CoordinatorPhase::Closed);
    assert_eq!(fx.advance().unwrap(), Advance::Idle);
    assert!(
        fx.flow
            .retire_state(
                &mut FlowRecording {
                    barrier: &mut fx.barrier,
                    inbox: &mut fx.inbox,
                    budget: &mut fx.budget,
                    author: &fx.author,
                },
                &mut fx.clock
            )
            .is_err()
    );
}

#[test]
fn retirement_refuses_held_completion_and_unexpected_eof_keeps_owned_evidence() {
    let mut fx = Fixture::new(4);
    fx.begin_control(s::StateControl::Shape {});
    fx.answer_control(s::StateControlAnswer::Shape(vec![]));
    fx.advance().unwrap();
    assert!(fx.flow.operation_completion().is_some());
    assert!(
        fx.flow
            .begin_releasing(&mut FlowRecording {
                barrier: &mut fx.barrier,
                inbox: &mut fx.inbox,
                budget: &mut fx.budget,
                author: &fx.author,
            })
            .is_err()
    );
    assert!(fx.flow.operation_completion().is_some());
    assert!(fx.flow.standing_state().is_some());

    let mut fx = Fixture::new(4);
    fx.flow
        .begin_releasing(&mut FlowRecording {
            barrier: &mut fx.barrier,
            inbox: &mut fx.inbox,
            budget: &mut fx.budget,
            author: &fx.author,
        })
        .unwrap();
    fx.state_peer.shutdown(std::net::Shutdown::Write).unwrap();
    assert!(
        fx.flow
            .retire_state(
                &mut FlowRecording {
                    barrier: &mut fx.barrier,
                    inbox: &mut fx.inbox,
                    budget: &mut fx.budget,
                    author: &fx.author,
                },
                &mut fx.clock
            )
            .is_err()
    );
    assert!(fx.flow.standing_state().is_some());
    assert_eq!(fx.flow.phase(), CoordinatorPhase::Failed);
}

#[test]
fn first_halt_freezes_the_actual_sample_once_and_preopen_has_no_synthetic_sample() {
    use crate::flow::report::{FlowHaltReason, HaltPoint, SecondaryFailure};
    let mut fx = Fixture::new(4);
    fx.admit(&["Read alpha."]);
    fx.advance().unwrap();
    let first = SecondaryFailure {
        reason: FlowHaltReason::ProtocolFailed,
        point: HaltPoint::Barrier,
        observed_ns: Some(fx.time().into()),
        detail: "first observed failure".into(),
    };
    fx.flow.latch(
        first.clone(),
        None,
        &mut FlowRecording {
            barrier: &mut fx.barrier,
            inbox: &mut fx.inbox,
            budget: &mut fx.budget,
            author: &fx.author,
        },
    );
    let sample = fx.flow.final_sample().unwrap().unwrap().clone();
    assert!(sample.pending_count.0 > 0);
    assert_eq!(fx.flow.halt().unwrap().first(), &first);
    assert!(fx.budget.is_frozen());
    fx.flow.latch(
        SecondaryFailure {
            detail: "later".into(),
            ..first.clone()
        },
        Some(0),
        &mut FlowRecording {
            barrier: &mut fx.barrier,
            inbox: &mut fx.inbox,
            budget: &mut fx.budget,
            author: &fx.author,
        },
    );
    assert_eq!(fx.flow.final_sample().unwrap(), Some(&sample));
    assert_eq!(fx.flow.halt().unwrap().first(), &first);
    assert_eq!(
        fx.flow.halt().unwrap().deadline().unwrap(),
        first.observed_ns.unwrap().0 + 150_000_000_000
    );
    let mut boot = BootFixture::new();
    boot.flow.latch(
        first,
        None,
        &mut FlowRecording {
            barrier: &mut boot.barrier,
            inbox: &mut boot.inbox,
            budget: &mut boot.budget,
            author: &boot.author,
        },
    );
    assert!(boot.flow.final_sample().unwrap().is_none());
    assert!(boot.flow.observations.is_none());
}

fn held_classifier_admission(
    refuse: bool,
) -> (
    BootFixture<crate::flow::ste100_socket::SocketPort>,
    Option<crate::flow::ste100_channel::Channel>,
    crate::flow::ste100_socket::AdmissionPoll,
) {
    use crate::flow::ste100_channel::{Channel, Receive, Tag};
    use crate::flow::ste100_socket::{AdmissionPoll, SocketPort};
    use nix::sys::socket::{AddressFamily, SockFlag, SockType, socketpair};
    let seed: s::SeedRequest = s::decode_strict(include_str!("fixtures/seed.json")).unwrap();
    let scheme = s::SchemeDeclaration {
        scheme_id: seed.scheme_id.clone(),
        artifact_sha256: s::Digest::new(seed.scheme_id.strip_prefix("ste100:").unwrap()).unwrap(),
        verbs: vec!["read".into()],
    };
    let bootstrap = s::Ste100Bootstrap {
        protocol: s::STE100_PROTOCOL.into(),
        scheme_text: seed.ste100_scheme,
        instruction: s::Ste100Instruction {
            model_binding: weaver_types::ModelBinding {
                artifact: weaver_types::ArtifactRef("synthetic-admission-fixture".into()),
                devices: vec![weaver_types::DeviceOrdinal(0)],
            },
            model_artifact_sha256: seed.checker_artifact_sha256,
            instances: 1,
            max_wire_bytes: 262144.into(),
            measurement_ref: "synthetic transport only, not qualification".into(),
            repair_cap: s::RepairCap {
                attempts: 2,
                measurement_ref: "fixture".into(),
            },
            decision_wait: s::MeasuredWait {
                elapsed_ns: 1000000.into(),
                measurement_ref: "fixture".into(),
            },
        },
    };
    let expected = if refuse {
        AdmissionPoll::Refused(s::Ste100AdmissionRefused {
            protocol: s::STE100_PROTOCOL.into(),
            code: s::Ste100AdmissionCode::UnsupportedArtifact,
            detail: "fixture artifact refused".into(),
        })
    } else {
        AdmissionPoll::Ready(s::Ste100Ready {
            protocol: s::STE100_PROTOCOL.into(),
            scheme_id: scheme.scheme_id.clone(),
            artifact_sha256: scheme.artifact_sha256.clone(),
            checker_artifact_sha256: bootstrap.instruction.model_artifact_sha256.clone(),
            instances: 1,
            max_wire_bytes: bootstrap.instruction.max_wire_bytes,
        })
    };
    let mut fx = BootFixture::<SocketPort>::with_scheme(scheme);
    fx.begin_and_answer_open();
    fx.load_and_activate();
    let (end, peer) = socketpair(
        AddressFamily::Unix,
        SockType::SeqPacket,
        None,
        SockFlag::SOCK_CLOEXEC,
    )
    .unwrap();
    let mut peer = Some(Channel::from_owned(peer, 262144).unwrap());
    fx.flow
        .begin_classifier_admission(
            &mut FlowRecording {
                barrier: &mut fx.barrier,
                inbox: &mut fx.inbox,
                budget: &mut fx.budget,
                author: &fx.author,
            },
            end,
            bootstrap.clone(),
        )
        .unwrap();
    let mut received = false;
    for _ in 0..64 {
        assert_eq!(
            fx.flow
                .advance_classifier_admission(&mut FlowRecording {
                    barrier: &mut fx.barrier,
                    inbox: &mut fx.inbox,
                    budget: &mut fx.budget,
                    author: &fx.author,
                })
                .unwrap(),
            AdmissionAdvance::Pending
        );
        if let Receive::Complete(message) = peer.as_mut().unwrap().try_receive().unwrap() {
            assert_eq!(message.tag, Tag::Bootstrap);
            assert_eq!(
                s::decode_strict::<s::Ste100Bootstrap>(
                    std::str::from_utf8(&message.payload).unwrap()
                )
                .unwrap(),
                bootstrap
            );
            received = true;
            break;
        }
    }
    assert!(received);
    let (tag, bytes) = match &expected {
        AdmissionPoll::Ready(ready) => (Tag::Ready, serde_json::to_vec(ready).unwrap()),
        AdmissionPoll::Refused(refusal) => {
            (Tag::AdmissionRefused, serde_json::to_vec(refusal).unwrap())
        }
        AdmissionPoll::Pending => unreachable!(),
    };
    peer.as_mut().unwrap().begin_send(tag, 0, bytes).unwrap();
    for _ in 0..64 {
        peer.as_mut().unwrap().try_send().unwrap();
        if !peer.as_ref().unwrap().has_pending_send() {
            break;
        }
    }
    assert!(!peer.as_ref().unwrap().has_pending_send());
    for _ in 0..64 {
        if fx
            .flow
            .advance_classifier_admission(&mut FlowRecording {
                barrier: &mut fx.barrier,
                inbox: &mut fx.inbox,
                budget: &mut fx.budget,
                author: &fx.author,
            })
            .unwrap()
            == AdmissionAdvance::Held
        {
            break;
        }
    }
    assert_eq!(fx.flow.held_admission(), Some(&expected));
    assert!(
        fx.flow.standing_classifier().is_none(),
        "transport must not grant service"
    );
    assert!(fx.flow.starting_classifier().is_some());
    (fx, peer, expected)
}

#[test]
fn admission_transport_holds_ready_and_refusal_until_one_explicit_take() {
    for (refuse, close_before_take) in [(false, false), (true, false), (false, true)] {
        let (mut fx, mut peer, expected) = held_classifier_admission(refuse);
        if refuse {
            peer.take();
        }
        assert_eq!(
            fx.flow
                .advance_classifier_admission(&mut FlowRecording {
                    barrier: &mut fx.barrier,
                    inbox: &mut fx.inbox,
                    budget: &mut fx.budget,
                    author: &fx.author,
                })
                .unwrap(),
            AdmissionAdvance::Held
        );
        if close_before_take {
            peer.take();
            assert!(
                fx.flow
                    .take_admission_completion(&mut FlowRecording {
                        barrier: &mut fx.barrier,
                        inbox: &mut fx.inbox,
                        budget: &mut fx.budget,
                        author: &fx.author,
                    })
                    .is_err()
            );
            assert_eq!(fx.flow.held_admission(), Some(&expected));
            assert!(fx.flow.standing_classifier().is_none());
            assert!(fx.flow.starting_classifier().is_some());
            continue;
        }
        assert_eq!(
            fx.flow
                .take_admission_completion(&mut FlowRecording {
                    barrier: &mut fx.barrier,
                    inbox: &mut fx.inbox,
                    budget: &mut fx.budget,
                    author: &fx.author,
                })
                .unwrap(),
            Some(expected)
        );
        assert!(
            fx.flow
                .take_admission_completion(&mut FlowRecording {
                    barrier: &mut fx.barrier,
                    inbox: &mut fx.inbox,
                    budget: &mut fx.budget,
                    author: &fx.author,
                })
                .unwrap()
                .is_none()
        );
        assert_eq!(fx.flow.standing_classifier().is_some(), !refuse);
        assert_eq!(fx.flow.admission_refused(), refuse);
        assert!(fx.flow.failure().is_none());
        if refuse {
            assert_eq!(
                fx.flow
                    .advance_classifier_admission(&mut FlowRecording {
                        barrier: &mut fx.barrier,
                        inbox: &mut fx.inbox,
                        budget: &mut fx.budget,
                        author: &fx.author,
                    })
                    .unwrap(),
                AdmissionAdvance::Pending
            );
            assert!(fx.flow.failure().is_none());
            fx.flow
                .retire_refused_classifier(&mut FlowRecording {
                    barrier: &mut fx.barrier,
                    inbox: &mut fx.inbox,
                    budget: &mut fx.budget,
                    author: &fx.author,
                })
                .unwrap();
            assert!(fx.flow.starting_classifier().is_none());
            assert!(fx.flow.standing_classifier().is_none());
            assert!(fx.flow.classifier_retired());
            assert!(fx.flow.failure().is_none());
            assert!(
                fx.flow
                    .retire_refused_classifier(&mut FlowRecording {
                        barrier: &mut fx.barrier,
                        inbox: &mut fx.inbox,
                        budget: &mut fx.budget,
                        author: &fx.author,
                    })
                    .is_err(),
                "intentional retirement cannot be consumed twice"
            );
        }
    }
}

#[test]
fn held_ready_waits_for_the_exact_pending_author_before_granting_service() {
    let (mut fx, peer, expected) = held_classifier_admission(false);
    let large = "x".repeat(32768);
    let mut filled = false;
    for _ in 0..ORDINARY_CAPACITY + 64 {
        let progress = fx.barrier.submit_authored_with_clock(
            authored(
                Kind::ModelMeasurement,
                AuthoredSource::Runtime,
                Payload::ModelMeasurement(
                    serde_json::value::to_raw_value(&serde_json::json!({"fixture":large})).unwrap(),
                ),
                true,
            ),
            RecordPurpose::Observation,
            &fx.author,
            100,
            &mut fx.inbox,
            &mut fx.budget,
            &mut || Ok(123),
        );
        match progress {
            RecordProgress::Recorded { .. } => {}
            RecordProgress::Pending => {
                filled = true;
                break;
            }
            other => panic!("fill trace: {other:?}, {:?}", fx.barrier.failure()),
        }
    }
    assert!(filled, "fixture must reach actual trace capacity");
    let pending = serde_json::to_string(fx.barrier.pending().unwrap()).unwrap();
    let ticket = format!("{:?}", fx.barrier.pending_author().unwrap());
    for _ in 0..3 {
        assert!(
            fx.flow
                .take_admission_completion(&mut FlowRecording {
                    barrier: &mut fx.barrier,
                    inbox: &mut fx.inbox,
                    budget: &mut fx.budget,
                    author: &fx.author,
                })
                .unwrap()
                .is_none()
        );
        assert_eq!(fx.flow.held_admission(), Some(&expected));
        assert!(fx.flow.starting_classifier().is_some());
        assert!(fx.flow.standing_classifier().is_none());
        assert_eq!(
            serde_json::to_string(fx.barrier.pending().unwrap()).unwrap(),
            pending
        );
        assert_eq!(
            format!("{:?}", fx.barrier.pending_author().unwrap()),
            ticket
        );
    }
    let mut sink_reader = fx.reader.take().unwrap();
    let drain = std::thread::spawn(move || {
        let mut bytes = Vec::new();
        sink_reader.read_to_end(&mut bytes).unwrap();
        bytes
    });
    let deadline = Instant::now() + Duration::from_secs(8);
    loop {
        match fx
            .barrier
            .resume_with_clock(&mut fx.inbox, &mut fx.budget, &mut || Ok(123))
        {
            RecordProgress::Recorded { .. } => break,
            RecordProgress::Pending => {
                assert!(Instant::now() < deadline, "pending author did not record");
                std::thread::yield_now();
            }
            other => panic!("pending author: {other:?}"),
        }
    }
    assert!(fx.barrier.pending().is_none());
    assert!(fx.barrier.pending_author().is_none());
    assert_eq!(
        fx.flow
            .take_admission_completion(&mut FlowRecording {
                barrier: &mut fx.barrier,
                inbox: &mut fx.inbox,
                budget: &mut fx.budget,
                author: &fx.author,
            })
            .unwrap(),
        Some(expected)
    );
    assert!(fx.flow.held_admission().is_none());
    assert!(fx.flow.starting_classifier().is_none());
    assert!(fx.flow.standing_classifier().is_some());
    assert!(
        fx.flow
            .take_admission_completion(&mut FlowRecording {
                barrier: &mut fx.barrier,
                inbox: &mut fx.inbox,
                budget: &mut fx.budget,
                author: &fx.author,
            })
            .unwrap()
            .is_none()
    );
    assert!(fx.flow.failure().is_none());
    drop(fx);
    drop(peer);
    assert!(!drain.join().unwrap().is_empty());
}

#[test]
fn first_halt_sample_counts_admitted_copies_before_the_ledger_can_drain() {
    use crate::flow::report::{FlowHaltReason, HaltPoint, SecondaryFailure};
    let mut fx = Fixture::new(4);
    fx.admit(&["Read queued.", "Read second."]);
    let intake_ns = fx.time();
    let expected_bytes: u64 = queued_sources(&fx.inbox)
        .unwrap()
        .iter()
        .map(|fact| fact.raw_bytes + fact.context_bytes)
        .sum();
    assert_eq!(fx.inbox.queued().count(), 1);
    assert!(fx.flow.ledger().facts().next().is_none());
    fx.elapse(17);
    let now = fx.time();
    fx.flow.latch(
        SecondaryFailure {
            reason: FlowHaltReason::TraceFailed,
            point: HaltPoint::Author,
            observed_ns: Some(now.into()),
            detail: "trace failed after admitting copy".into(),
        },
        None,
        &mut FlowRecording {
            barrier: &mut fx.barrier,
            inbox: &mut fx.inbox,
            budget: &mut fx.budget,
            author: &fx.author,
        },
    );
    let sample = fx.flow.final_sample().unwrap().unwrap();
    assert_eq!(sample.pending_count.0, 2);
    assert_eq!(sample.pending_bytes.0, expected_bytes);
    assert_eq!(sample.pending_turns.0, 1);
    assert_eq!(sample.oldest_age_ns.0, now - intake_ns);
    assert_eq!(sample.retained_blocks.0, 2);
    assert_eq!(sample.retained_bytes.0, fx.budget.gauges().1);
    assert!(fx.flow.ledger().facts().next().is_none());
    assert_eq!(
        fx.inbox.queued().count(),
        1,
        "reporting cannot drain the frozen inbox"
    );
}

#[test]
fn a_recorded_operation_stays_owned_until_a_later_pending_author_is_recorded() {
    let mut fx = Fixture::new(4);
    fx.begin_control(s::StateControl::Shape {});
    fx.answer_control(s::StateControlAnswer::Shape(vec![s::ShapeRow {
        run: "prior".into(),
        kind: "message.user".into(),
        count: 17.into(),
    }]));
    fx.advance().unwrap();
    let Some(OperationCompletion::State { completion, .. }) = fx.flow.operation_completion() else {
        panic!("actual recorded control answer");
    };
    let expected = s::encode_state_line(&completion.answer, s::FrameDirection::Response).unwrap();
    let pending = fx.fill_trace_capacity();
    for _ in 0..3 {
        assert!(fx.take_operation().unwrap().is_none());
        assert_eq!(
            serde_json::to_string(fx.barrier.pending().unwrap()).unwrap(),
            pending
        );
        let Some(OperationCompletion::State { completion, .. }) = fx.flow.operation_completion()
        else {
            panic!("held answer must remain owned");
        };
        assert_eq!(
            s::encode_state_line(&completion.answer, s::FrameDirection::Response).unwrap(),
            expected
        );
    }
    let reader = fx.start_drain();
    fx.resume_external_ticket();
    let Some(OperationCompletion::State { completion, .. }) = fx.take_operation().unwrap() else {
        panic!("operation follows external Recorded");
    };
    assert_eq!(
        s::encode_state_line(&completion.answer, s::FrameDirection::Response).unwrap(),
        expected
    );
    assert!(fx.take_operation().unwrap().is_none());
    drop(fx);
    reader.join().unwrap();
}

#[test]
fn preactivation_refusal_rollback_preserves_unused_open_without_a_serving_proof() {
    for premature_eof in [false, true] {
        let mut fx = BootFixture::new();
        fx.begin_and_answer_open();
        let held = fx.flow.state().held_measurement().unwrap().clone();
        fx.load_only();
        let mut invalid = fixture::partition();
        invalid.rules[0].kind = "save_point".into();
        assert!(validate_partition(&invalid).is_err());
        let refusal = authored(
            Kind::Refusal,
            AuthoredSource::Runtime,
            Payload::Refusal(
                serde_json::value::RawValue::from_string(r#"{"case":"partition_invalid"}"#.into())
                    .unwrap(),
            ),
            false,
        );
        let progress = fx.barrier.submit_authored_with_clock(
            refusal,
            RecordPurpose::Observation,
            &fx.author,
            100,
            &mut fx.inbox,
            &mut fx.budget,
            &mut || Ok(122),
        );
        assert!(
            matches!(progress, RecordProgress::Recorded { .. }),
            "refusal: {progress:?}"
        );
        assert!(fx.barrier.is_refused());
        assert!(!fx.barrier.is_active());
        assert!(fx.flow.halt().is_none());
        if premature_eof {
            fx.peer.shutdown(std::net::Shutdown::Write).unwrap();
        }
        let result = fx.flow.begin_enter_rollback(
            &mut FlowRecording {
                barrier: &mut fx.barrier,
                inbox: &mut fx.inbox,
                budget: &mut fx.budget,
                author: &fx.author,
            },
            &mut fx.clock,
        );
        if premature_eof {
            assert!(result.is_err());
            assert_eq!(fx.flow.state().held_measurement(), Some(&held));
            assert!(fx.flow.unused_open().is_none());
            assert!(fx.flow.operation_completion().is_none());
            continue;
        }
        assert_eq!(result.unwrap(), Advance::Progress);
        assert_eq!(fx.flow.phase(), CoordinatorPhase::Releasing);
        assert!(fx.flow.standing_state().is_none());
        assert!(fx.flow.standing_classifier().is_none());
        assert!(fx.flow.classifier_retired());
        assert_eq!(fx.flow.unused_open().unwrap().measurement(), &held);
        assert!(matches!(
            fx.flow.unused_open().unwrap().answer(),
            StateFrame::Opened { .. }
        ));
        assert!(fx.flow.operation_completion().is_none());
        assert!(fx.flow.observations.is_none());
        assert!(fx.flow.final_sample().unwrap().is_none());
        assert!(fx.flow.halt().is_none());
        assert!(fx.flow.ledger().facts().next().is_none());
        assert_eq!(fx.budget.gauges(), (0, 0));
        let mut byte = [0];
        assert_eq!(
            fx.peer.read(&mut byte).unwrap(),
            0,
            "rollback closes the sole state fd"
        );
        let unload = trace::UnloadClose {
            grant_surface: None,
            cause: Some(fixture::load_record().cause),
            forced: true,
            forced_by: None,
            release: None,
            end_reason: None,
            state_flow_failure: None,
        };
        let progress = fx.barrier.submit_authored_with_clock(
            authored(
                Kind::Unload,
                AuthoredSource::Runtime,
                Payload::Unload(Box::new(unload)),
                false,
            ),
            RecordPurpose::Observation,
            &fx.author,
            100,
            &mut fx.inbox,
            &mut fx.budget,
            &mut || Ok(123),
        );
        assert!(
            matches!(progress, RecordProgress::Recorded { .. }),
            "unload: {progress:?}"
        );
        fx.flow
            .close(&mut FlowRecording {
                barrier: &mut fx.barrier,
                inbox: &mut fx.inbox,
                budget: &mut fx.budget,
                author: &fx.author,
            })
            .unwrap();
        assert_eq!(fx.flow.phase(), CoordinatorPhase::Closed);
        assert_eq!(fx.flow.unused_open().unwrap().measurement(), &held);
        let mut reader = BufReader::new(fx.reader.take().unwrap());
        let mut kinds = Vec::new();
        for _ in 0..3 {
            let mut line = String::new();
            reader.read_line(&mut line).unwrap();
            kinds.push(serde_json::from_str::<serde_json::Value>(&line).unwrap()["kind"].clone());
        }
        assert_eq!(kinds, vec!["load", "refusal", "unload"]);
    }
}

#[test]
fn enter_rollback_requires_a_recorded_refusal_and_never_replaces_clean_retirement() {
    let mut fx = BootFixture::new();
    fx.begin_and_answer_open();
    fx.load_only();
    assert!(
        fx.flow
            .begin_enter_rollback(
                &mut FlowRecording {
                    barrier: &mut fx.barrier,
                    inbox: &mut fx.inbox,
                    budget: &mut fx.budget,
                    author: &fx.author,
                },
                &mut fx.clock
            )
            .is_err()
    );
    assert!(fx.flow.state().held_measurement().is_some());
    assert!(fx.flow.unused_open().is_none());
    let mut active = Fixture::new(4);
    assert!(
        active
            .flow
            .begin_enter_rollback(
                &mut FlowRecording {
                    barrier: &mut active.barrier,
                    inbox: &mut active.inbox,
                    budget: &mut active.budget,
                    author: &active.author,
                },
                &mut active.clock
            )
            .is_err()
    );
    assert!(active.flow.standing_state().is_some());
    assert!(active.flow.unused_open().is_none());
}

// A second applicable trait makes this expression ambiguous if Clone appears.
const _: fn() = || {
    trait RecordedCheckMustNotClone<Marker> {
        fn check() {}
    }
    impl<T: ?Sized> RecordedCheckMustNotClone<()> for T {}
    impl<T: Clone> RecordedCheckMustNotClone<u8> for T {}
    let _ = <RecordedCheck as RecordedCheckMustNotClone<_>>::check;
};

#[test]
fn recorded_check_retains_exact_source_across_a_later_pending_author() {
    let mut fx = Fixture::new(4);
    let operation = template_operation();
    assert!(matches!(
        fx.begin_check(operation.clone()),
        OperationStart::Started { .. }
    ));
    let expected_reply = fx.reply_check(s::Ste100Result::Accepted { bindings: vec![] });
    for _ in 0..128 {
        fx.advance().unwrap();
        if fx.flow.operation_completion().is_some() {
            break;
        }
    }
    let result = fx
        .records()
        .into_iter()
        .find(|record| record["kind"] == "ste100.result")
        .expect("real classifier result must be recorded");
    let expected_source = s::SourcePosition {
        block_id: s::SourceBlockId {
            run: "run".into(),
            sequence: result["sequence"]
                .as_str()
                .unwrap()
                .parse::<u64>()
                .unwrap()
                .into(),
            block: 0.into(),
        },
        turn: None,
    };
    assert_eq!(
        result["payload"],
        serde_json::to_value(&expected_reply).unwrap()
    );
    assert!(
        result["turn"].is_null(),
        "load template replies are turnless"
    );
    let Some(OperationCompletion::Check(held)) = fx.flow.operation_completion() else {
        panic!("actual recorded check is held");
    };
    assert_eq!(held.source(), &expected_source);
    assert_eq!(held.operation(), &operation);
    assert_eq!(held.reply(), &expected_reply);

    let pending = fx.fill_trace_capacity();
    let later = fx.records().last().unwrap()["sequence"]
        .as_str()
        .unwrap()
        .parse::<u64>()
        .unwrap();
    assert!(
        later > expected_source.block_id.sequence.0,
        "later actual authors must distinguish proof source from last sequence"
    );
    for _ in 0..3 {
        assert!(fx.take_operation().unwrap().is_none());
        assert_eq!(fx.advance().unwrap(), Advance::Pending);
        assert_eq!(
            serde_json::to_string(fx.barrier.pending().unwrap()).unwrap(),
            pending
        );
        let Some(OperationCompletion::Check(held)) = fx.flow.operation_completion() else {
            panic!("later author cannot consume the exact held check");
        };
        assert_eq!(held.source(), &expected_source);
    }
    let drain = fx.start_drain();
    fx.resume_external_ticket();
    let OperationCompletion::Check(recorded) = fx.take_operation().unwrap().unwrap() else {
        panic!("one recorded check transfers after the pending author");
    };
    let (actual_operation, reply, source) = recorded.into_parts();
    assert_eq!(actual_operation, operation);
    assert_eq!(reply, expected_reply);
    assert_eq!(source, expected_source);
    assert!(fx.take_operation().unwrap().is_none());
    assert_eq!(
        fx.records()
            .iter()
            .filter(|r| r["kind"] == "ste100.result")
            .count(),
        1
    );
    drop(fx);
    drain.join().unwrap();
}
