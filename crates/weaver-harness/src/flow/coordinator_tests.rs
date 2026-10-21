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

struct Fixture {
    flow: FlowCoordinator<ScriptedPort>,
    classifier: ScriptedPeer,
    state_peer: UnixStream,
    state_bytes: Vec<u8>,
    requests: Vec<s::StateFrame>,
    classifier_replies: usize,
    operation: u64,
    barrier: RecordBarrier,
    inbox: FlowInbox,
    budget: RetentionBudget,
    author: ServingAuthor,
    clock: FakePumpIo,
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

fn operation(request: &s::StateFrame, id: u64, began: u64) -> s::StateOperationObservation {
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
    fn new(max_blocks: u64) -> Self {
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
                instrument_version: "state-observation/1".into(),
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

    fn time(&mut self) -> u64 {
        self.clock.now().unwrap()
    }
    fn elapse(&mut self, delta: u64) {
        let now = self.time();
        self.clock.advance_to(now + delta).unwrap();
    }

    fn advance(&mut self) -> Result<Advance, FlowFailure> {
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

    fn transport(&mut self) -> Result<(), FlowFailure> {
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

    fn admit(&mut self, texts: &[&str]) -> Vec<SourceBlockId> {
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

    fn collect_requests(&mut self) {
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

    fn answer_classifier(&mut self, reject: bool) {
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

    fn answer_file(&mut self, index: usize, wrong_identity: bool) {
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

    fn until_file(&mut self) {
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

    fn records(&self) -> Vec<serde_json::Value> {
        self.barrier
            .recorder()
            .structure()
            .iter()
            .map(|record| serde_json::from_str(&record.line).unwrap())
            .collect()
    }

    fn start_drain(&mut self) -> std::thread::JoinHandle<Vec<u8>> {
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
