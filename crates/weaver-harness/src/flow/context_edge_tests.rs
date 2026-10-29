//! Context ownership edges use actual trace, state and classifier transports.
//! The synthetic clock and language answers establish mechanics only.

use super::super::author::AuthoredSource;
use super::super::clock::{WaitInterest, WaitOutcome};
use super::super::coordinator::{AdmissionAdvance, TransportSource};
use super::super::halt_snapshot::ExternalFacts;
use super::super::pump::LifecyclePorts;
use super::super::report::{CaptureAtHalt, CaptureState, ReportError};
use super::super::ste100_channel::{Channel, Receive, Tag};
use super::super::ste100_socket::AdmissionPoll;
use super::tests::{Fixture, config, event};
use super::*;
use nix::sys::socket::{AddressFamily, SockFlag, SockType, socketpair};
use std::io;
use std::os::fd::AsRawFd;
use weaver_trace::{Subsystem, TurnRef};
use weaver_types::state_flow as s;
use weaver_types::{ArtifactRef, DeviceOrdinal, ModelBinding};

/// Expire at a particular ownership boundary without sleeping. A real wait
/// would be a fixture defect: these tests give every needed reply in advance.
struct ExpireAfterReads {
    remaining: usize,
    before: u64,
    expired: u64,
}

impl PumpIo for ExpireAfterReads {
    fn now(&mut self) -> io::Result<u64> {
        if self.remaining == 0 {
            Ok(self.expired)
        } else {
            self.remaining -= 1;
            Ok(self.before)
        }
    }

    fn wait(&mut self, _: &[WaitInterest<'_>], _: Option<u64>) -> io::Result<WaitOutcome> {
        panic!("this fixture has no external work left to wait for");
    }
}

fn ports<I: PumpIo>(io: &mut I) -> PumpPorts<'_, I> {
    PumpPorts {
        lifecycle: LifecyclePorts::default(),
        gate: None,
        decode: None,
        label: None,
        io,
    }
}

fn statement(text: &str) -> AuthoredEvent {
    let message = serde_json::json!({
        "role": "assistant", "content": [{"type": "text", "text": text}],
    });
    AuthoredEvent {
        kind: Kind::MessageAssistant,
        subsystem: Subsystem::Harness,
        turn: Some(TurnRef("turn".into())),
        source: AuthoredSource::Model,
        causal_parent: Some(Sequence(0)),
        payload: Some(Payload::Message(
            serde_json::value::RawValue::from_string(message.to_string()).unwrap(),
        )),
    }
}

fn statement_bytes(authored: &AuthoredEvent) -> &str {
    let Some(Payload::Message(message)) = &authored.payload else {
        panic!("the original statement payload was lost");
    };
    message.get()
}

#[test]
fn final_drain_leave_wait_preserves_queued_records_for_successful_retry() {
    use super::super::clock::{MonotonicIo, testing::FakePumpIo};
    use nix::sys::socket::{setsockopt, sockopt::SndBuf};
    use std::io::Read;
    use std::os::unix::net::UnixStream;
    use std::time::{Duration, Instant};

    // Expiry at pump entry and inside its deadline check both return Wait.
    for remaining in [1, 2] {
        let (sink, reader) = UnixStream::pair().unwrap();
        setsockopt(&sink, SndBuf, &4096).unwrap();
        sink.set_write_timeout(Some(Duration::from_secs(5)))
            .unwrap();
        reader
            .set_read_timeout(Some(Duration::from_secs(5)))
            .unwrap();
        let (state, peer) = UnixStream::pair().unwrap();
        peer.set_read_timeout(Some(Duration::from_secs(3))).unwrap();
        peer.set_write_timeout(Some(Duration::from_secs(3)))
            .unwrap();
        let mut clock = FakePumpIo::new(100);
        let context = RecordingContext::admit(
            sink.into(),
            StateChannel::new(state, "session".into(), "run".into()).unwrap(),
            "session".into(),
            "run".into(),
            42,
            config(),
            AbsoluteBound::after(100, 1_000_000_000).unwrap(),
            &mut clock,
        )
        .unwrap();
        let mut fx = Fixture {
            context,
            clock,
            state: peer,
            _reader: reader,
        };
        fx.activate();
        fx.context.complete_wait(PumpDeadline::Enter).unwrap();
        let prefix_len = fx
            .context
            .barrier
            .recorder()
            .structure()
            .iter()
            .map(|record| record.line.len())
            .sum();
        let mut prefix = vec![0; prefix_len];
        fx._reader.read_exact(&mut prefix).unwrap();
        let watch = Instant::now() + Duration::from_secs(3);
        while fx.context.barrier.recorder().pressure().queued != 0 {
            assert!(Instant::now() < watch, "initial records did not drain");
            std::thread::yield_now();
        }

        // This complete untapped record exceeds the real socket's capacity.
        // Later shutdown records must stay queued while its write is blocked.
        fx.author(event(
            Kind::Fault,
            Some(Payload::Fault(
                serde_json::value::RawValue::from_string(format!(
                    "{{\"fixture\":\"{}\"}}",
                    "x".repeat(131_072)
                ))
                .unwrap(),
            )),
        ))
        .unwrap();
        let leave = AbsoluteBound::after(104, 5).unwrap();
        fx.context.begin_releasing(leave, &mut fx.clock).unwrap();
        assert_eq!(
            fx.context.retire_state(&mut fx.clock).unwrap(),
            Advance::Progress
        );
        assert_eq!(
            fx.context.retire_classifier(&mut fx.clock).unwrap(),
            Advance::Progress
        );
        fx.author(event(Kind::SessionClosed, None)).unwrap();
        let unload = fx
            .author(event(
                Kind::Unload,
                Some(Payload::Unload(Box::new(serving::payloads::UnloadClose {
                    grant_surface: None,
                    cause: None,
                    forced: false,
                    forced_by: None,
                    release: serving::payloads::Release {
                        spu: None,
                        member: None,
                    },
                    end_reason: None,
                    state_flow_failure: None,
                }))),
            ))
            .unwrap();
        assert_eq!(fx.context.barrier.recorder().pressure().queued, 3);
        let expected: Vec<u8> = fx
            .context
            .barrier
            .recorder()
            .structure()
            .iter()
            .flat_map(|record| record.line.as_bytes().iter().copied())
            .collect();
        let mut expiry = ExpireAfterReads {
            remaining,
            before: 104,
            expired: leave.deadline_ns(),
        };
        assert_eq!(
            fx.context.close_after_drain(&mut ports(&mut expiry)),
            Err(RunFailure::Wait {
                key: PumpDeadline::Leave,
                bound: leave,
            })
        );
        assert_eq!(fx.context.phase(), RecordingPhase::Releasing);
        assert_eq!(fx.context.flow.phase(), CoordinatorPhase::Closed);
        assert_eq!(fx.context.barrier.recorder().pressure().queued, 3);
        assert!(fx.context.failure().is_none());
        assert!(fx.context.halt().is_none());
        assert_eq!(fx.context.deadlines.get(PumpDeadline::Leave), Some(leave));

        // Acknowledge only the expired lifecycle wait, then resume these owners.
        assert_eq!(fx.context.complete_wait(PumpDeadline::Leave), Some(leave));
        let mut reader = fx._reader.try_clone().unwrap();
        let remaining_bytes = expected.len() - prefix.len();
        let drain = std::thread::spawn(move || {
            let mut bytes = vec![0; remaining_bytes];
            reader.read_exact(&mut bytes).map(|()| bytes)
        });
        let result = fx.context.close_after_drain(&mut ports(&mut MonotonicIo));
        let received = drain.join().unwrap();
        result.unwrap();
        prefix.extend(received.expect("retry discarded queued trace records"));
        assert_eq!(prefix, expected);
        assert_eq!(fx.context.phase(), RecordingPhase::Closed);
        assert_eq!(
            fx.context.barrier.recorder().boundary().committed,
            Some(unload)
        );
        assert_eq!(fx.context.barrier.recorder().pressure().queued, 0);
    }
}

#[test]
fn recorded_author_result_survives_a_post_record_lifecycle_deadline_once() {
    let mut fx = Fixture::new(config());
    fx.activate();
    fx.context.complete_wait(PumpDeadline::Enter).unwrap();
    let bound = AbsoluteBound::after(104, 5).unwrap();
    fx.context
        .arm(PumpDeadline::Label, bound, &mut fx.clock)
        .unwrap();
    let before = fx.context.barrier.recorder().structure().len();
    // The first read checks the live owner and the second prepares the author.
    // The next read follows actual trace admission of this untapped event.
    let mut io = ExpireAfterReads {
        remaining: 2,
        before: 104,
        expired: 109,
    };
    assert_eq!(
        fx.context
            .author_and_wait(&mut ports(&mut io), event(Kind::SessionClosed, None))
            .map_err(|failure| failure.retained_reason())
            .unwrap_err(),
        RunFailure::Wait {
            key: PumpDeadline::Label,
            bound,
        }
    );
    assert_eq!(fx.context.barrier.recorder().structure().len(), before + 1);
    assert!(fx.context.pending_author().is_none());
    assert!(fx.context.waiting_content.is_none());
    assert_eq!(fx.context.completed_author, Some(Sequence(before as u64)));
    assert_eq!(fx.context.phase(), RecordingPhase::Entered);
    assert!(fx.context.flow.halt().is_none());

    fx.context.complete_wait(PumpDeadline::Label).unwrap();
    fx.clock.advance_to(109).unwrap();
    assert_eq!(
        fx.context
            .resume_author_and_wait(&mut ports(&mut fx.clock))
            .map_err(|failure| failure.retained_reason())
            .unwrap(),
        Sequence(before as u64)
    );
    assert!(fx.context.completed_author.is_none());
    assert_eq!(fx.context.barrier.recorder().structure().len(), before + 1);
    assert_eq!(
        fx.author(event(Kind::SessionClosed, None)).unwrap(),
        Sequence(before as u64 + 1)
    );
    assert!(matches!(
        fx.context
            .resume_author_and_wait(&mut ports(&mut fx.clock))
            .map_err(|failure| failure.retained_reason()),
        Err(RunFailure::Flow {
            reason: FlowHaltReason::ProtocolFailed,
            ..
        })
    ));
    assert_eq!(fx.context.barrier.recorder().structure().len(), before + 2);
}

#[test]
fn an_unreturned_recorded_result_blocks_a_second_author() {
    let mut fx = Fixture::new(config());
    fx.activate();
    fx.context.complete_wait(PumpDeadline::Enter).unwrap();
    let bound = AbsoluteBound::after(104, 5).unwrap();
    fx.context
        .arm(PumpDeadline::Label, bound, &mut fx.clock)
        .unwrap();
    let mut io = ExpireAfterReads {
        remaining: 2,
        before: 104,
        expired: 109,
    };
    assert!(matches!(
        fx.context
            .author_and_wait(&mut ports(&mut io), event(Kind::SessionClosed, None))
            .map_err(|failure| failure.retained_reason()),
        Err(RunFailure::Wait { .. })
    ));
    let completed = fx.context.completed_author;
    let count = fx.context.barrier.recorder().structure().len();
    fx.context.complete_wait(PumpDeadline::Label).unwrap();
    fx.clock.advance_to(109).unwrap();
    assert!(matches!(
        fx.author(statement("Read second.")),
        Err(RunFailure::Flow {
            reason: FlowHaltReason::ProtocolFailed,
            ..
        })
    ));
    assert_eq!(fx.context.completed_author, completed);
    assert_eq!(fx.context.barrier.recorder().structure().len(), count);
    assert_eq!(
        statement_bytes(fx.context.refused_author().unwrap()),
        statement_bytes(&statement("Read second."))
    );
}

pub(super) fn classifier_fixture() -> (s::Ste100Bootstrap, s::SchemeDeclaration, s::Ste100Ready) {
    let seed: s::SeedRequest = s::decode_strict(include_str!("fixtures/seed.json")).unwrap();
    let scheme = s::SchemeDeclaration {
        artifact_sha256: s::Digest::new(seed.scheme_id.strip_prefix("ste100:").unwrap()).unwrap(),
        scheme_id: seed.scheme_id,
        verbs: vec!["read".into()],
    };
    let instruction = s::Ste100Instruction {
        model_binding: ModelBinding {
            artifact: ArtifactRef("/synthetic/context-manifest.json".into()),
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

pub(super) fn activate_classifier_duty(fx: &mut Fixture, bootstrap: &s::Ste100Bootstrap) {
    fx.opened();
    let mut authored = fx.load_event();
    let Some(Payload::Elections(load)) = &mut authored.payload else {
        unreachable!();
    };
    load.ste100_scheme = bootstrap.scheme_text.clone();
    load.state_flow.ste100 = super::super::mirror::instruction(&bootstrap.instruction);
    load.template_inventory.checker_artifact_sha256 =
        bootstrap.instruction.model_artifact_sha256.as_str().into();
    assert_eq!(fx.author(authored).unwrap(), Sequence(0));
    fx.context.activate(&mut fx.clock).unwrap();
    for _ in 0..16 {
        fx.context.reduce_flow(&mut fx.clock).unwrap();
        if fx.context.flow.operation_completion().is_some() {
            break;
        }
    }
    assert!(matches!(
        fx.context.take_operation_completion(&mut fx.clock).unwrap(),
        Some(OperationCompletion::Opened { .. })
    ));
}

fn flush_peer(peer: &mut Channel) {
    for _ in 0..64 {
        peer.try_send().unwrap();
        if !peer.has_pending_send() {
            return;
        }
    }
    panic!("fixture peer could not send its complete frame");
}

fn attach_classifier(
    fx: &mut Fixture,
    bootstrap: s::Ste100Bootstrap,
    ready: s::Ste100Ready,
) -> Channel {
    let (end, peer) = socketpair(
        AddressFamily::Unix,
        SockType::SeqPacket,
        None,
        SockFlag::SOCK_CLOEXEC,
    )
    .unwrap();
    let mut peer = Channel::from_owned(peer, ready.max_wire_bytes.0 as usize).unwrap();
    fx.context
        .begin_classifier(end, bootstrap.clone(), &mut fx.clock)
        .unwrap();
    let mut received = false;
    for _ in 0..64 {
        assert_eq!(
            fx.context
                .flow
                .advance_classifier_admission(&mut FlowRecording {
                    barrier: &mut fx.context.barrier,
                    inbox: &mut fx.context.inbox,
                    budget: &mut fx.context.budget,
                    author: &fx.context.author,
                })
                .unwrap(),
            AdmissionAdvance::Pending
        );
        if let Receive::Complete(frame) = peer.try_receive().unwrap() {
            assert_eq!((frame.tag, frame.id), (Tag::Bootstrap, 0));
            let received_bootstrap: s::Ste100Bootstrap =
                s::decode_strict(std::str::from_utf8(&frame.payload).unwrap()).unwrap();
            assert_eq!(received_bootstrap, bootstrap);
            received = true;
            break;
        }
    }
    assert!(received);
    peer.begin_send(Tag::Ready, 0, serde_json::to_vec(&ready).unwrap())
        .unwrap();
    flush_peer(&mut peer);
    assert_eq!(
        fx.context
            .flow
            .advance_classifier_admission(&mut FlowRecording {
                barrier: &mut fx.context.barrier,
                inbox: &mut fx.context.inbox,
                budget: &mut fx.context.budget,
                author: &fx.context.author,
            })
            .unwrap(),
        AdmissionAdvance::Held
    );
    assert_eq!(
        fx.context.take_classifier_admission(&mut fx.clock).unwrap(),
        Some(AdmissionPoll::Ready(ready))
    );
    peer
}

fn classifier_transport(fx: &mut Fixture) {
    fx.context
        .flow
        .transport_one(
            &mut FlowRecording {
                barrier: &mut fx.context.barrier,
                inbox: &mut fx.context.inbox,
                budget: &mut fx.context.budget,
                author: &fx.context.author,
            },
            &mut fx.clock,
            TransportSource::Classifier,
        )
        .unwrap();
}

#[test]
fn a_backlog_wait_resumes_the_original_author_before_it_had_a_ticket() {
    let (bootstrap, scheme, ready) = classifier_fixture();
    let mut declaration = config();
    declaration.scheme = scheme;
    declaration.backlog.max_pending_blocks = 1.into();
    let mut fx = Fixture::new(declaration);
    activate_classifier_duty(&mut fx, &bootstrap);
    let mut peer = attach_classifier(&mut fx, bootstrap, ready);
    fx.context.complete_wait(PumpDeadline::Enter).unwrap();
    let first = fx.author(statement("Read first.")).unwrap();
    let mut request = None;
    for _ in 0..64 {
        fx.context.reduce_flow(&mut fx.clock).unwrap();
        classifier_transport(&mut fx);
        if let Receive::Complete(frame) = peer.try_receive().unwrap() {
            assert_eq!(frame.tag, Tag::Request);
            let value: s::Ste100Request =
                s::decode_strict(std::str::from_utf8(&frame.payload).unwrap()).unwrap();
            assert_eq!(frame.id, value.request_id.0);
            request = Some(value);
            break;
        }
    }
    let request = request.expect("first source reached the concrete classifier socket");
    assert!(matches!(&request.target, s::Ste100Target::Source(source)
        if source.sequence.0 == first.0));
    let second = fx.author(statement("Read second.")).unwrap();
    assert!(fx.context.budget.waiting_copy().is_some());
    let count = fx.context.barrier.recorder().structure().len();
    let original = statement("Read third.");
    let expected_bytes = statement_bytes(&original).to_owned();
    let bound = AbsoluteBound::after(104, 5).unwrap();
    fx.context
        .arm(PumpDeadline::Label, bound, &mut fx.clock)
        .unwrap();
    let mut io = ExpireAfterReads {
        remaining: 1,
        before: 104,
        expired: 109,
    };
    assert_eq!(
        fx.context
            .author_and_wait(&mut ports(&mut io), original)
            .map_err(|failure| failure.retained_reason())
            .unwrap_err(),
        RunFailure::Wait {
            key: PumpDeadline::Label,
            bound,
        }
    );
    assert!(fx.context.pending_author().is_none());
    let kept = fx.context.waiting_content.as_ref().unwrap();
    assert_eq!(statement_bytes(kept), expected_bytes);
    assert!(matches!(kept.source, AuthoredSource::Model));
    assert_eq!(kept.causal_parent, Some(Sequence(0)));
    assert_eq!(kept.turn, Some(TurnRef("turn".into())));
    assert_eq!(fx.context.barrier.recorder().structure().len(), count);
    assert!(fx.context.failure().is_none());

    // The actual classifier result is recorded before its terminal disposition
    // releases the first copy's charge. No synthetic receipt releases backlog.
    let reply = s::Ste100Reply {
        request_id: request.request_id,
        target: request.target,
        scheme_id: request.scheme_id,
        result: s::Ste100Result::NotSte100 {
            code: s::Ste100DefectCode::NonSte100,
            detail: "synthetic rejection used to release this first source".into(),
            spans: Vec::new(),
        },
    };
    peer.begin_send(
        Tag::Reply,
        reply.request_id.0,
        serde_json::to_vec(&reply).unwrap(),
    )
    .unwrap();
    flush_peer(&mut peer);
    fx.context.complete_wait(PumpDeadline::Label).unwrap();
    fx.clock.advance_to(109).unwrap();
    classifier_transport(&mut fx);
    assert!(
        fx.context
            .flow
            .standing_classifier()
            .unwrap()
            .held()
            .is_some()
    );
    let third = fx
        .context
        .resume_author_and_wait(&mut ports(&mut fx.clock))
        .map_err(|failure| failure.retained_reason())
        .unwrap();
    assert!(third.0 > second.0);
    assert!(fx.context.waiting_content.is_none());
    assert!(fx.context.pending_author().is_none());
    assert!(fx.context.failure().is_none());
    let serving::AdmittedCopy::Statement(waiting) = fx.context.budget.waiting_copy().unwrap()
    else {
        panic!("a statement became a pair copy");
    };
    assert_eq!(waiting.blocks[0].source.sequence, third);
    assert_eq!(waiting.blocks.len(), 1);
    assert_eq!(
        waiting.blocks[0].raw.as_ref(),
        r#"{"text":"Read third.","type":"text"}"#
    );
    let sources: Vec<_> = fx
        .context
        .flow
        .ledger()
        .facts()
        .map(|fact| fact.source.sequence.0)
        .collect();
    assert_eq!(sources, vec![second.0]);
    assert_eq!(fx.context.budget.gauges().0, 1);
}

#[test]
fn bootstrap_must_match_the_recorded_load_before_any_classifier_bytes_are_sent() {
    for change_instruction in [false, true] {
        let (mut bootstrap, scheme, _) = classifier_fixture();
        let mut declaration = config();
        declaration.scheme = scheme;
        let mut fx = Fixture::new(declaration);
        activate_classifier_duty(&mut fx, &bootstrap);
        let count = fx.context.barrier.recorder().structure().len();
        if change_instruction {
            bootstrap.instruction.max_wire_bytes.0 += 1;
        } else {
            bootstrap.scheme_text.push('\n');
        }
        let (end, peer) = socketpair(
            AddressFamily::Unix,
            SockType::SeqPacket,
            None,
            SockFlag::SOCK_CLOEXEC,
        )
        .unwrap();
        assert!(matches!(
            fx.context.begin_classifier(end, bootstrap, &mut fx.clock),
            Err(RunFailure::Flow {
                reason: FlowHaltReason::ProtocolFailed,
                point: HaltPoint::Barrier,
                ..
            })
        ));
        assert!(fx.context.flow.starting_classifier().is_none());
        assert!(fx.context.flow.standing_classifier().is_none());
        assert_eq!(fx.context.barrier.recorder().structure().len(), count);
        let mut bytes = [0; 128];
        assert_eq!(
            nix::sys::socket::recv(
                peer.as_raw_fd(),
                &mut bytes,
                nix::sys::socket::MsgFlags::MSG_DONTWAIT
            )
            .unwrap(),
            0,
            "refusal closed the supplied end without sending bootstrap bytes"
        );
    }
}

type CorruptLoad = fn(&mut serving::payloads::LoadRecord);

#[test]
fn load_refuses_structurally_valid_facts_that_differ_from_its_actual_owners() {
    let cases: &[(&str, CorruptLoad)] = &[
        ("template scheme", |load| {
            load.template_inventory.scheme_id.push_str("-other")
        }),
        ("template checker", |load| {
            load.template_inventory.checker_artifact_sha256 = "d".repeat(64)
        }),
        ("partition identity", |load| {
            load.state_flow.partition[0].id.push_str("-other")
        }),
        ("gate measurement", |load| {
            load.state_flow.gate_wait.measurement_ref.push_str("-other")
        }),
        ("cut bound", |load| {
            load.state_flow.cut_wait.elapsed_ns.0 += 1
        }),
        ("observation interval", |load| {
            load.state_flow.observation_interval.elapsed_ns.0 += 1
        }),
        ("backlog allowance", |load| {
            load.state_flow.backlog.max_pending_bytes.0 += 1
        }),
        ("verb dictionary", |load| {
            load.state_flow.scheme.verbs = vec!["write".into()]
        }),
        ("read allowance", |load| {
            load.state_flow.reads.max_result_bytes.0 += 1
        }),
        ("read measurement", |load| {
            load.state_flow.reads.measurement_ref.push_str("-other")
        }),
        ("dump binding", |load| {
            load.state_start.dump_sha256 = "c".repeat(64)
        }),
        ("member identity", |load| {
            load.state_member_observation.pid += 1
        }),
        ("member start", |load| {
            load.state_member_observation.start_ticks = Some(11.into())
        }),
    ];
    for (name, corrupt) in cases {
        let mut fx = Fixture::new(config());
        fx.opened();
        let mut authored = fx.load_event();
        let Some(Payload::Elections(load)) = &mut authored.payload else {
            unreachable!();
        };
        assert!(fx.context.load_matches(load), "baseline fixture: {name}");
        corrupt(load);
        let expected = serde_json::to_vec(load).unwrap();
        assert!(
            matches!(
                fx.author(authored),
                Err(RunFailure::Flow {
                    reason: FlowHaltReason::ProtocolFailed,
                    point: HaltPoint::Author,
                    ..
                })
            ),
            "{name}"
        );
        assert!(!fx.context.barrier.is_loaded(), "{name}");
        assert!(!fx.context.barrier.is_active(), "{name}");
        assert!(fx.context.barrier.pending_author().is_none(), "{name}");
        assert!(
            fx.context.barrier.admitted_before_failure().is_none(),
            "{name}"
        );
        let mut trace_bytes = [0; 128];
        assert_eq!(
            nix::sys::socket::recv(
                fx._reader.as_raw_fd(),
                &mut trace_bytes,
                nix::sys::socket::MsgFlags::MSG_DONTWAIT
            )
            .unwrap_err(),
            nix::errno::Errno::EAGAIN,
            "a rejected preflight load produced trace bytes: {name}"
        );
        let Some(Payload::Elections(kept)) = &fx.context.refused_author().unwrap().payload else {
            panic!("refused load was lost: {name}");
        };
        assert_eq!(serde_json::to_vec(kept).unwrap(), expected, "{name}");
    }
}

fn external(wait: Option<WaitAtHalt>, input_id: Option<s::DecimalU64>) -> ExternalFacts {
    ExternalFacts {
        outstanding_pairs: Vec::new(),
        capture: None,
        wait,
        input_id,
    }
}

#[test]
fn halt_account_matches_the_latched_wait_and_installs_the_frozen_owners_once() {
    let mut fx = Fixture::new(config());
    fx.activate();
    let source = fx.author(statement("Read owed.")).unwrap();
    fx.context.complete_wait(PumpDeadline::Enter).unwrap();
    let bound = AbsoluteBound::after(104, 5).unwrap();
    fx.context
        .arm(PumpDeadline::Gate, bound, &mut fx.clock)
        .unwrap();
    fx.clock.advance_to(109).unwrap();
    assert!(matches!(
        fx.author(event(Kind::SessionClosed, None)),
        Err(RunFailure::Flow {
            reason: FlowHaltReason::GateBound,
            ..
        })
    ));
    let wait = fx.context.halt_wait().unwrap().clone();
    assert_eq!((wait.began_ns.0, wait.deadline_ns.0), (104, 109));
    assert_eq!(wait.stage, serving::payloads::BlockedStage::Gate);
    assert!(fx.context.budget.is_frozen());
    let before = fx.context.budget.gauges();

    let mut wrong_wait = wait.clone();
    // Still an observed expiry, but not this owner's original absolute bound.
    // The generic account validator alone cannot establish this identity.
    wrong_wait.deadline_ns.0 -= 1;
    let capture = CaptureAtHalt {
        capture_id: 17.into(),
        state: CaptureState::AwaitPublished,
        commit_sent: true,
        published: None,
    };
    let mut wrong = external(Some(wrong_wait.clone()), None);
    wrong.capture = Some(capture.clone());
    let HaltAccountFailure::Snapshot(failure) = fx.context.freeze_halt_account(wrong).unwrap_err()
    else {
        panic!("wrong wait reached account installation");
    };
    assert_eq!(failure.external.wait, Some(wrong_wait));
    assert_eq!(failure.external.capture, Some(capture));
    assert_eq!(failure.external.input_id, None);
    assert_eq!(
        failure.error,
        ReportError::Invalid("wait differs from latched bound")
    );
    assert!(fx.context.halt().unwrap().account().is_none());

    fx.context
        .freeze_halt_account(external(Some(wait.clone()), None))
        .unwrap();
    let installed = fx.context.halt().unwrap().account().unwrap();
    assert_eq!(installed.wait, Some(wait.clone()));
    assert_eq!(installed.reason, FlowHaltReason::GateBound);
    assert_eq!(installed.outstanding_blocks.len(), 1);
    assert_eq!(
        installed.outstanding_blocks[0].block_id.sequence.0,
        source.0
    );
    let original = installed.to_raw().unwrap().get().to_owned();
    let HaltAccountFailure::Install(failure) = fx
        .context
        .freeze_halt_account(external(Some(wait), None))
        .unwrap_err()
    else {
        panic!("second account was not rejected by the one-install owner");
    };
    assert_eq!(failure.error, ReportError::SnapshotAlreadyInstalled);
    assert_eq!(failure.account.input_id, None);
    assert_eq!(
        fx.context
            .halt()
            .unwrap()
            .account()
            .unwrap()
            .to_raw()
            .unwrap()
            .get(),
        original
    );
    assert_eq!(fx.context.budget.gauges(), before);
    assert_eq!(fx.context.flow.ledger().facts().count(), 1);
}
