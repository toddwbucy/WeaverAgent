//! Actual diagnostic submissions, with conversion and writer custody checked separately.
use super::*;
use s::payloads as p;
use std::fs::File;
use std::os::unix::fs::FileExt;
use weaver_trace::{self as t, serving as s};

const RAW: &str = r#"{ "held":"line\n\u00e9", "number":1.00 }"#;
fn raw() -> Box<serde_json::value::RawValue> {
    serde_json::value::RawValue::from_string(RAW.into()).unwrap()
}

struct Diagnostic {
    record: Record,
    sink: File,
}
impl Diagnostic {
    fn new() -> Self {
        let sink = scratch_file("diagnostic-conversion");
        let record = Record::from(
            weaver_diagnostic::Recorder::receive(
                sink.try_clone().unwrap().into(),
                weaver_diagnostic::RunRef("diagnostic-run".into()),
                weaver_diagnostic::SessionRef("diagnostic-session".into()),
            )
            .unwrap(),
        );
        Self { record, sink }
    }
    fn bytes(&self) -> String {
        let mut bytes = vec![0; self.sink.metadata().unwrap().len() as usize];
        self.sink.read_exact_at(&mut bytes, 0).unwrap();
        String::from_utf8(bytes).unwrap()
    }
    fn assert_refused(&mut self, event: s::Event) {
        let before = self.bytes();
        let refused = self.record.submit_shared_diagnostic(event);
        assert!(
            matches!(refused, Err(RecordFailure::OutsideDiagnosticVocabulary)),
            "{refused:?}"
        );
        assert!(self.record.failed(), "the refusal must remain latched");
        assert_eq!(self.bytes(), before, "conversion refusal wrote to the sink");
    }
    fn assert_malformed(&mut self, event: s::Event) {
        let before = self.bytes();
        assert!(matches!(
            self.record.submit_shared_diagnostic(event),
            Err(RecordFailure::Diagnostic(
                weaver_diagnostic::Failure::SubmitRefused {
                    refusal: weaver_diagnostic::SubmitRefusal::PayloadKindMismatch
                }
            ))
        ));
        assert!(self.record.failed());
        assert_eq!(self.bytes(), before);
    }
    fn assert_writer_sound_without_clearing_the_record_latch(&mut self) {
        assert!(self.record.failed());
        // This is a writer-mechanism probe, not permission to resume a failed
        // run. Record intentionally exposes the sticky latch to its caller.
        // No latch field or arm is changed to make the writer accept the probe.
        let sequence = self
            .record
            .submit(legacy(Kind::Fault, Some(t::Payload::Fault(raw()))))
            .unwrap();
        assert_eq!(
            sequence,
            Sequence(0),
            "a refused conversion consumed a sequence"
        );
        assert!(
            self.record.failed(),
            "a sound write cannot repair the failed run"
        );
        assert_eq!(self.bytes().lines().count(), 1);
    }
}

fn legacy(kind: Kind, payload: Option<t::Payload>) -> Event {
    Event {
        envelope: t::Envelope {
            session: t::SessionRef("source-session".into()),
            run: t::RunRef("source-run".into()),
            turn: Some(t::TurnRef("source-turn".into())),
            sequence: Sequence(71),
            kind,
            subsystem: Subsystem::Harness,
            causal_parent: Some(Sequence(17)),
            wall_ms: 12345,
            monotonic_ns: t::MonotonicNs(67890),
        },
        payload,
    }
}
fn shared(kind: s::Kind, payload: Option<s::Payload>) -> s::Event {
    s::Event {
        envelope: s::Envelope {
            session: t::SessionRef("source-session".into()),
            run: t::RunRef("source-run".into()),
            turn: Some(t::TurnRef("source-turn".into())),
            sequence: Sequence(71),
            kind,
            channel: s::Channel::Measurement,
            origin: s::Origin::Runtime,
            subsystem: Subsystem::Harness,
            causal_parent: Some(Sequence(17)),
            wall_ms: 12345,
            monotonic_ns: t::MonotonicNs(67890),
        },
        payload,
    }
}

fn compatible() -> Vec<(Event, s::Event)> {
    let mut cases = vec![(
        legacy(Kind::TurnStarted, None),
        shared(s::Kind::TurnStarted, None),
    )];
    macro_rules! case {
        ($kind:ident, $payload:ident, $value:expr) => {{
            let value = $value;
            cases.push((
                legacy(Kind::$kind, Some(t::Payload::$payload(value.clone()))),
                shared(s::Kind::$kind, Some(s::Payload::$payload(value))),
            ));
        }};
    }
    case!(TurnClosed, TurnClosed, t::TurnClose::Clean);
    case!(MessageSystem, Message, raw());
    case!(MessageUser, Message, raw());
    case!(MessageAssistant, Message, raw());
    case!(MessageToolResult, Message, raw());
    case!(ModelRequest, ModelRequest, raw());
    case!(
        ModelOutput,
        ModelOutput,
        t::ModelOutput {
            emission: "original output\n\u{e9}".into(),
            finish: t::Finish::Length,
            resident: 33,
            capacity: 4096,
        }
    );
    case!(ModelMeasurement, ModelMeasurement, raw());
    case!(
        ModelField,
        ModelField,
        t::ModelField {
            position: 9,
            ranked: vec![t::Candidate {
                token: 42,
                probability: 0.75
            }],
            realized: 0,
        }
    );
    case!(
        Flush,
        Flush,
        t::FlushCounts {
            resident_before: 31,
            resident_after: 7
        }
    );
    case!(Refusal, Refusal, raw());
    case!(Fault, Fault, raw());
    case!(
        Elision,
        Elision,
        t::ElisionSpan {
            from: 3,
            to: 8,
            resident_before: 31,
            resident_after: 26
        }
    );
    case!(ModelUnrecoveredCall, UnrecoveredCall, raw());
    for (old, new) in &mut cases {
        if matches!(old.envelope.kind, Kind::Flush | Kind::Elision) {
            old.envelope.turn = None;
            new.envelope.turn = None;
        }
    }
    cases
}

#[test]
fn diagnostic_shared_conversion_preserves_every_legacy_member_and_canonical_payload() {
    let cases = compatible();
    assert_eq!(cases.len(), 15);
    for (old, new) in cases {
        // Conversion preserves input identities/sequence. The real diagnostic
        // writer then binds its own run/session and allocates its own sequence.
        let expected = serde_json::to_value(&old).unwrap();
        assert_eq!(
            serde_json::to_value(convert(old.clone()).unwrap()).unwrap(),
            expected
        );
        assert_eq!(
            serde_json::to_value(convert_shared(new.clone()).unwrap()).unwrap(),
            expected
        );
        let mut old_sink = Diagnostic::new();
        let mut new_sink = Diagnostic::new();
        assert_eq!(old_sink.record.submit(old).unwrap(), Sequence(0));
        assert_eq!(
            new_sink.record.submit_shared_diagnostic(new).unwrap(),
            Sequence(0)
        );
        assert_eq!(old_sink.bytes(), new_sink.bytes());
        assert!(!old_sink.record.failed());
        assert!(!new_sink.record.failed());
        let mut expected = expected;
        expected["session"] = "diagnostic-session".into();
        expected["run"] = "diagnostic-run".into();
        expected["sequence"] = "0".into();
        assert_eq!(
            serde_json::from_str::<serde_json::Value>(&new_sink.bytes()).unwrap(),
            expected
        );
    }
}

#[test]
fn diagnostic_mapping_omits_only_channel_origin_and_keeps_raw_spelling() {
    let mut fixture = Diagnostic::new();
    let mut event = shared(s::Kind::MessageUser, Some(s::Payload::Message(raw())));
    event.envelope.channel = s::Channel::Input;
    event.envelope.origin = s::Origin::Operator;
    fixture
        .record
        .submit_shared_diagnostic(event.clone())
        .unwrap();
    assert_eq!(
        fixture.bytes(),
        concat!(
            "{\"session\":\"diagnostic-session\",\"run\":\"diagnostic-run\",\"turn\":\"source-turn\",",
            "\"sequence\":\"0\",\"kind\":\"message.user\",\"subsystem\":\"harness\",",
            "\"causal_parent\":\"17\",\"wall_ms\":12345,\"monotonic_ns\":\"67890\",",
            "\"payload\":{ \"held\":\"line\\n\\u00e9\", \"number\":1.00 }}\n"
        )
    );
    event.envelope.origin = s::Origin::External;
    event.envelope.causal_parent = None;
    event.envelope.turn = None;
    event.envelope.kind = s::Kind::MessageSystem;
    fixture.record.submit_shared_diagnostic(event).unwrap();
    let lines = fixture.bytes();
    let line: serde_json::Value = serde_json::from_str(lines.lines().nth(1).unwrap()).unwrap();
    assert!(line.get("channel").is_none());
    assert!(line.get("origin").is_none());
    assert!(line.get("causal_parent").is_none());
    assert!(line.get("turn").is_none());
    assert_eq!(line["sequence"], "1");
    assert!(lines.lines().nth(1).unwrap().contains(RAW));
}

#[test]
fn diagnostic_legacy_recall_keeps_its_own_payload_and_spelling() {
    let mut fixture = Diagnostic::new();
    let mut event = legacy(
        Kind::Recall,
        Some(t::Payload::Recall(t::RecallAccount {
            ask: t::RecallAsk {
                verb: t::RecallVerb::Replay,
                last_turns: Some(3),
            },
            returned: vec![t::RecalledIdentity {
                run: "held-run".into(),
                turn: Some("held-turn".into()),
                sequence: "19".into(),
                kind: "message.user".into(),
            }],
            count: Some(1),
        })),
    );
    event.envelope.turn = None;
    let expected = serde_json::to_string(event.payload.as_ref().unwrap()).unwrap();
    fixture.record.submit(event).unwrap();
    assert!(fixture.bytes().contains(&expected));
    assert!(
        fixture
            .bytes()
            .contains(r#""ask":{"verb":"replay","last_turns":3}"#)
    );
    assert!(!fixture.record.failed());
}

#[test]
fn diagnostic_rejects_mismatched_legacy_and_shared_payloads_before_writing() {
    for (mut old, mut new) in compatible() {
        old.payload = Some(t::Payload::Deferred(raw()));
        let mut fixture = Diagnostic::new();
        assert!(matches!(
            fixture.record.submit(old),
            Err(RecordFailure::Diagnostic(
                weaver_diagnostic::Failure::SubmitRefused {
                    refusal: weaver_diagnostic::SubmitRefusal::PayloadKindMismatch
                }
            ))
        ));
        assert!(fixture.record.failed());
        assert!(fixture.bytes().is_empty());
        fixture.assert_writer_sound_without_clearing_the_record_latch();
        new.payload = Some(if new.envelope.kind == s::Kind::Fault {
            s::Payload::Message(raw())
        } else {
            s::Payload::Fault(raw())
        });
        let mut fixture = Diagnostic::new();
        fixture.assert_malformed(new);
        fixture.assert_writer_sound_without_clearing_the_record_latch();
    }
}

#[test]
fn diagnostic_shared_payload_required_refuses_before_the_writer() {
    let cases = compatible();
    let mut checked = 0;
    for (_, mut event) in cases {
        if event.envelope.kind == s::Kind::TurnStarted {
            continue;
        }
        event.payload = None;
        let missing_payload = |result| match result {
            Err(RecordFailure::Diagnostic(weaver_diagnostic::Failure::SubmitRefused {
                refusal: weaver_diagnostic::SubmitRefusal::RequiredFieldAbsent { field },
            })) => assert_eq!(field.0, "payload"),
            other => panic!("missing payload was not refused: {other:?}"),
        };
        // This assertion pins conversion's own boundary, even though the
        // diagnostic writer independently refuses a missing required payload.
        missing_payload(convert_shared(event.clone()).map(|_| ()));
        let mut fixture = Diagnostic::new();
        missing_payload(fixture.record.submit_shared_diagnostic(event).map(|_| ()));
        assert!(fixture.record.failed());
        assert!(fixture.bytes().is_empty());
        fixture.assert_writer_sound_without_clearing_the_record_latch();
        checked += 1;
    }
    assert_eq!(checked, 14);
}

#[test]
fn diagnostic_writer_replaces_converted_source_identity_and_sequence_on_each_submit() {
    let mut fixture = Diagnostic::new();
    for (written, source) in [71, 5].into_iter().enumerate() {
        let mut event = shared(s::Kind::Fault, Some(s::Payload::Fault(raw())));
        event.envelope.sequence = Sequence(source);
        let converted = convert_shared(event.clone()).unwrap();
        assert_eq!(converted.envelope.session.0, "source-session");
        assert_eq!(converted.envelope.run.0, "source-run");
        assert_eq!(converted.envelope.sequence.0, source);
        assert_eq!(converted.envelope.causal_parent.unwrap().0, 17);
        assert_eq!(
            fixture.record.submit_shared_diagnostic(event).unwrap(),
            Sequence(written as u64)
        );
    }
    let bytes = fixture.bytes();
    let events: Vec<serde_json::Value> = bytes
        .lines()
        .map(|line| serde_json::from_str(line).unwrap())
        .collect();
    assert_eq!(events.len(), 2);
    for (written, event) in events.iter().enumerate() {
        assert_eq!(event["session"], "diagnostic-session");
        assert_eq!(event["run"], "diagnostic-run");
        assert_eq!(event["sequence"], written.to_string());
        assert_eq!(event["causal_parent"], "17");
        assert_eq!(event["turn"], "source-turn");
        assert_eq!(event["wall_ms"], 12345);
        assert_eq!(event["monotonic_ns"], "67890");
        assert!(event.get("channel").is_none());
        assert!(event.get("origin").is_none());
    }
    assert!(!fixture.record.failed());
}

#[test]
fn diagnostic_rejects_every_unshared_registry_kind_before_a_splice() {
    let compatible: Vec<_> = compatible()
        .into_iter()
        .map(|(_, event)| event.envelope.kind)
        .collect();
    let mut refused = 0;
    for kind in s::Kind::ALL {
        if compatible.contains(kind) {
            continue;
        }
        // These names cannot be diagnostic regardless of a payload's shape.
        // The typed recall/save/unload families are exercised separately.
        let mut fixture = Diagnostic::new();
        fixture.assert_refused(shared(*kind, Some(s::Payload::Message(raw()))));
        fixture.assert_writer_sound_without_clearing_the_record_latch();
        refused += 1;
    }
    assert_eq!(refused + compatible.len(), s::Kind::ALL.len());
    assert!(refused >= 15);
}

fn new_payloads() -> Vec<(s::Kind, s::Payload)> {
    let span = p::SpanRef {
        seed_id: "0123456789abcdef0123456789abcdef".into(),
        local_id: 0.into(),
        revision: 1.into(),
    };
    let source = p::ContentRef::Seed {
        seed_id: span.seed_id.clone(),
        ordinal: 0.into(),
    };
    let window = p::SaveWindow {
        began_ns: 10.into(),
        deadline_ns: 20.into(),
    };
    let dump = "a".repeat(64);
    let config = "b".repeat(64);
    let mut result = vec![
        (
            s::Kind::Recall,
            s::Payload::RecallRecord(Box::new(p::RecallRecord::Read {
                selector: p::ReadSelector::History {
                    noun: "file".into(),
                    verb: None,
                    condition: None,
                },
                snapshot_id: 1.into(),
                sources: vec![],
                spans: vec![],
            })),
        ),
        (
            s::Kind::Recall,
            s::Payload::RecallRecord(Box::new(p::RecallRecord::Boot {
                selector: p::ReadSelector::SystemPrompt {},
                snapshot_id: 1.into(),
                sources: vec![p::RecallSource {
                    source: source.clone(),
                    binding_ordinals: vec![0.into()],
                }],
                spans: vec![p::RecallSpan {
                    source,
                    selected_span: p::RecallSelection {
                        span_ref: span.clone(),
                        start: 0.into(),
                        end: 10.into(),
                    },
                }],
                message: serde_json::value::RawValue::from_string(
                    r#"{"role":"system","content":[{"type":"text","text":"Read file."}]}"#.into(),
                )
                .unwrap(),
                end_mark: p::RecallEndMark {
                    span,
                    end: 10.into(),
                },
            })),
        ),
        (
            s::Kind::SavePoint,
            s::Payload::SavePointRecord(Box::new(p::SavePointRecord::Requested {
                capture_id: 1.into(),
                window,
                tail_window: None,
                cause: p::PersistenceCause::Requested,
            })),
        ),
        (
            s::Kind::SavePoint,
            s::Payload::SavePointRecord(Box::new(p::SavePointRecord::Confirmed {
                capture_id: 1.into(),
                window,
                tail_window: None,
                cause: p::PersistenceCause::Requested,
                artifact_id: "artifact".into(),
                dump_sha256: dump.clone(),
                config_sha256: config.clone(),
                location: "save.ndjson".into(),
                position: None,
            })),
        ),
        (
            s::Kind::SavePoint,
            s::Payload::SavePointRecord(Box::new(p::SavePointRecord::Published {
                capture_id: 1.into(),
                window,
                tail_window: None,
                cause: p::PersistenceCause::Requested,
                artifact_id: "artifact".into(),
                dump_sha256: dump,
                config_sha256: config,
                location: "save.ndjson".into(),
                position: None,
            })),
        ),
        (
            s::Kind::SavePoint,
            s::Payload::SavePointRecord(Box::new(p::SavePointRecord::Refused {
                capture_id: 1.into(),
                window,
                tail_window: None,
                cause: p::PersistenceCause::Requested,
                refusal_code: "snapshot_timeout".into(),
            })),
        ),
        (
            s::Kind::LoopInstruction,
            s::Payload::LoopInstruction(Box::new(p::LoopInstructionRecord {
                template_id: "wind_down".into(),
                message: serde_json::value::RawValue::from_string(
                    r#"{"role":"user","content":[{"type":"text","text":"Close the step."}]}"#
                        .into(),
                )
                .unwrap(),
                instruction: p::LoopInstructionMeta::WindDown {},
            })),
        ),
    ];
    for (reason, failure) in [(false, false), (true, false), (false, true), (true, true)] {
        result.push((
            s::Kind::Unload,
            s::Payload::Unload(Box::new(p::UnloadClose {
                grant_surface: None,
                cause: None,
                forced: true,
                forced_by: None,
                release: None,
                end_reason: reason.then_some(p::EndReason::ComponentFailure),
                state_flow_failure: failure.then(|| p::StateFlowFailure {
                    fault_sequence: Some(9.into()),
                    account: raw(),
                }),
            })),
        ));
    }
    result
}

#[test]
fn diagnostic_new_recall_save_and_halt_payloads_refuse_even_under_a_legacy_kind() {
    let cases = new_payloads();
    assert_eq!(cases.len(), 11);
    for (kind, payload) in cases {
        for kind in [kind, s::Kind::Fault] {
            let mut fixture = Diagnostic::new();
            fixture.assert_refused(shared(kind, Some(payload.clone())));
            fixture.assert_writer_sound_without_clearing_the_record_latch();
        }
    }
}

#[test]
fn diagnostic_conversion_keeps_supported_subsystems_and_refuses_foreign_ones() {
    for subsystem in [Subsystem::Harness, Subsystem::Spu, Subsystem::SpuDecoder] {
        let mut fixture = Diagnostic::new();
        let mut event = shared(s::Kind::Fault, Some(s::Payload::Fault(raw())));
        event.envelope.subsystem = subsystem;
        fixture.record.submit_shared_diagnostic(event).unwrap();
        let line: serde_json::Value = serde_json::from_str(&fixture.bytes()).unwrap();
        assert_eq!(line["subsystem"], serde_json::to_value(subsystem).unwrap());
    }
    let mut fixture = Diagnostic::new();
    let mut event = shared(s::Kind::Fault, Some(s::Payload::Fault(raw())));
    event.envelope.subsystem = Subsystem::Gate;
    fixture.assert_refused(event);
    fixture.assert_writer_sound_without_clearing_the_record_latch();
}

#[test]
fn diagnostic_shared_door_cannot_submit_to_the_legacy_serving_arm() {
    for native in [false, true] {
        let mut record = Record::from(
            t::Recorder::receive(
                scratch_sink("new-diagnostic-door"),
                t::RunRef("r-1".into()),
                t::SessionRef("s-1".into()),
            )
            .unwrap(),
        );
        let event = shared(s::Kind::Fault, Some(s::Payload::Fault(raw())));
        let refused = if native {
            record
                .submit_diagnostic(convert_shared(event).unwrap())
                .map(|_| ())
        } else {
            record.submit_shared_diagnostic(event).map(|_| ())
        };
        assert!(matches!(
            refused,
            Err(RecordFailure::DiagnosticDoorOnServingRecord)
        ));
        assert!(record.failed());
        assert_eq!(record.structure().unwrap().by_kind(Kind::Fault).count(), 0);
        // The existing serving path still uses its own recorder. This checks
        // the unchanged mechanism while retaining the run's failed latch.
        assert_eq!(fault(&author(), &mut record).unwrap(), Sequence(0));
        record.drain().unwrap();
        assert_eq!(record.structure().unwrap().by_kind(Kind::Fault).count(), 1);
        assert!(record.failed());
    }
}
