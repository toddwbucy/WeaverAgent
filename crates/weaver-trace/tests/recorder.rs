//! conforms: trace-one-line-per-event
//! conforms: trace-load-carries-the-tee-election
//! conforms: trace-load-names-its-loop-and-its-member
//! conforms: trace-large-integers-as-decimal-strings
//! conforms: trace-admission-precedes-fan-out
//! conforms: trace-one-rendering-two-holders
//! conforms: trace-sequence-gapless
//! conforms: trace-bracket-kind-omits-payload
//! conforms: trace-envelope-flattens
//! conforms: trace-turn-close-internally-tagged
//! conforms: trace-output-carries-the-counts
//! conforms: trace-recall-records-the-ask-and-its-identities
//! conforms: trace-restored-message-is-turnless-and-whole
//! conforms: trace-score-records-the-verdict-and-its-terms
//!
//! Recorder tests of `weaver-trace-Spec` section 10. Verified removals are
//! named at the watches that hold them. Not every test names a perturbation.

use std::fs::File;
use std::io::Read;
use std::os::fd::OwnedFd;

use weaver_trace::{
    Envelope, Event, Failure, Kind, MonotonicNs, Payload, Recorder, RunRef, Sequence, SessionRef,
    StopReason, SubmitRefusal, Subsystem, TurnClose, TurnRef, raw_payload,
};

/// A path under the temp directory, removed when the test ends, pass or
/// fail: the guard drops on the unwind a failed assertion takes as on a clean
/// return, so no run leaves a sink behind (#690 item C2.9).
struct Scratch(std::path::PathBuf);

impl Drop for Scratch {
    fn drop(&mut self) {
        match std::fs::symlink_metadata(&self.0) {
            Ok(meta) if meta.is_dir() => drop(std::fs::remove_dir_all(&self.0)),
            Ok(_) => drop(std::fs::remove_file(&self.0)),
            Err(_) => {}
        }
    }
}

impl std::ops::Deref for Scratch {
    type Target = std::path::Path;
    fn deref(&self) -> &std::path::Path {
        &self.0
    }
}

impl AsRef<std::path::Path> for Scratch {
    fn as_ref(&self) -> &std::path::Path {
        &self.0
    }
}

fn sink() -> (OwnedFd, Scratch) {
    let path = Scratch(std::env::temp_dir().join(format!(
        "weaver-trace-test-{}-{:?}",
        std::process::id(),
        std::thread::current().id()
    )));
    let file = File::create(&path).expect("temp sink");
    (OwnedFd::from(file), path)
}

fn recorder() -> (Recorder, Scratch) {
    let (fd, path) = sink();
    let r =
        Recorder::receive(fd, RunRef("r-1".into()), SessionRef("s-1".into())).expect("receives");
    (r, path)
}

fn envelope(kind: Kind, turn: Option<&str>) -> Envelope {
    Envelope {
        session: SessionRef("s-1".into()),
        run: RunRef("r-1".into()),
        turn: turn.map(|t| TurnRef(t.into())),
        sequence: Sequence(0),
        kind,
        subsystem: Subsystem::Harness,
        causal_parent: None,
        wall_ms: 1_754_400_000_000,
        monotonic_ns: MonotonicNs(9_007_199_254_740_993),
    }
}

fn event(kind: Kind, turn: Option<&str>, payload: Option<Payload>) -> Event {
    Event {
        envelope: envelope(kind, turn),
        payload,
    }
}

/// The elections a load declares. Every `load` event carries them as of
/// 2026-08-21, so a record says what posture it was written in.
fn elections() -> Payload {
    Payload::Elections(weaver_trace::Elections {
        residual_readout: false,
        field: None,
        surprisal: false,
        tee: Some(weaver_trace::Election::default()),
        state_member: false,
        declaration: Default::default(),
        lineage: None,
        stack: Default::default(),
        state_store: Default::default(),
        composer: weaver_trace::LoopIdentity::compiled("test"),
    })
}

fn user_message(turn: &str) -> Event {
    event(
        Kind::MessageUser,
        Some(turn),
        Some(Payload::Message(
            raw_payload("{\"role\":\"user\",\"content\":[]}").unwrap(),
        )),
    )
}

/// One line per event: a payload carrying an embedded newline renders to one
/// line, serde escaping it to `\n`.
///
/// Perturbation: bypass the escaping by splicing octets that carry a raw
/// newline - `raw_payload` refuses the construction, which is the mechanism,
/// and hand-building the line instead was watched to split the stream.
#[test]
fn one_line_per_event() {
    let (mut r, path) = recorder();
    r.submit(event(Kind::Load, None, Some(elections())))
        .unwrap();
    let prose = "line one\nline two";
    let rendered =
        serde_json::to_string(&serde_json::json!({"role":"user","text":prose})).expect("renders");
    r.submit(event(
        Kind::MessageUser,
        Some("t-1"),
        Some(Payload::Message(raw_payload(&rendered).unwrap())),
    ))
    .unwrap();
    r.drain().unwrap();
    let mut out = String::new();
    File::open(&path).unwrap().read_to_string(&mut out).unwrap();
    assert_eq!(
        out.lines().count(),
        2,
        "two events, two lines, embedded newline escaped"
    );
}

/// A raw newline in submitted octets cannot enter, in either position: JSON
/// forbids one inside a string, and the separator check refuses one between
/// tokens, where pretty-printing legally puts it.
#[test]
fn raw_newline_octets_refuse_construction() {
    assert!(raw_payload("{\"text\":\"a\nb\"}").is_none());
    assert!(raw_payload("{\n  \"role\": \"user\"\n}").is_none());
    assert!(raw_payload("{\r\n\"role\":\"user\"}").is_none());
    assert!(
        raw_payload("{\"text\":\"a\\nb\"}").is_some(),
        "an escaped newline is two octets"
    );
}

/// The monotonic reading beyond the double-safe range serializes as a decimal
/// string and survives exactly.
///
/// Perturbation: render the reading as a bare number and a consumer parsing
/// doubles reads 9007199254740992 - one off, silently. Watched by asserting
/// on the string form, which the bare rendering fails.
#[test]
fn large_integers_render_as_decimal_strings() {
    let (mut r, path) = recorder();
    r.submit(event(Kind::Load, None, Some(elections())))
        .unwrap();
    r.drain().unwrap();
    let mut out = String::new();
    File::open(&path).unwrap().read_to_string(&mut out).unwrap();
    assert!(
        out.contains("\"monotonic_ns\":\"9007199254740993\""),
        "the reading is a decimal string: {out}"
    );
    assert!(
        out.contains("\"sequence\":\"0\""),
        "the sequence is a decimal string: {out}"
    );
    assert!(
        out.contains("\"wall_ms\":1754400000000"),
        "the wall clock stays bare: {out}"
    );
}

/// Admission precedes the fan-out: a refused submission leaves no record in
/// the structure and no line on the stream.
///
/// Perturbation: move the refusal after the append and a row appears for the
/// refused event. Watched under exactly that reordering.
#[test]
fn refused_submission_touches_neither_sink() {
    let (mut r, path) = recorder();
    r.submit(event(Kind::Load, None, Some(elections())))
        .unwrap();
    let err = r
        .submit(event(Kind::MessageUser, Some("t-1"), None))
        .unwrap_err();
    assert!(matches!(
        err,
        Failure::RefusedOnSubmit {
            reason: SubmitRefusal::RequiredFieldAbsent { .. }
        }
    ));
    assert_eq!(r.structure().len(), 1, "the refused event landed nowhere");
    r.drain().unwrap();
    let mut out = String::new();
    File::open(&path).unwrap().read_to_string(&mut out).unwrap();
    assert_eq!(
        out.lines().count(),
        1,
        "the stream holds only the admitted event"
    );
}

/// One rendering, two holders: the bytes in the structure are the bytes on
/// the stream.
///
/// Perturbation: re-render for the writer from the event and the comparison
/// fails the moment the two paths diverge. Watched by comparing structure
/// lines against the drained file byte for byte.
#[test]
fn structure_bytes_are_stream_bytes() {
    let (mut r, path) = recorder();
    r.submit(event(Kind::Load, None, Some(elections())))
        .unwrap();
    r.submit(user_message("t-1")).unwrap();
    r.submit(event(
        Kind::TurnClosed,
        Some("t-1"),
        Some(Payload::TurnClosed(TurnClose::Stopped {
            reason: StopReason::Directive,
        })),
    ))
    .unwrap();
    r.drain().unwrap();
    let held: String = r.structure().iter().map(|rec| rec.line.as_ref()).collect();
    let mut streamed = String::new();
    File::open(&path)
        .unwrap()
        .read_to_string(&mut streamed)
        .unwrap();
    assert_eq!(held, streamed, "one rendering reaches both holders");
}

/// The sequence is gapless over admitted events: a refused submission
/// consumes no sequence.
///
/// Perturbation: assign the sequence before admission and a refusal leaves a
/// gap. Watched under exactly that reordering.
#[test]
fn sequence_is_gapless_over_admitted_events() {
    let (mut r, _path) = recorder();
    let a = r
        .submit(event(Kind::Load, None, Some(elections())))
        .unwrap();
    let _ = r
        .submit(event(Kind::MessageUser, Some("t"), None))
        .unwrap_err();
    let b = r
        .submit(event(Kind::TurnStarted, Some("t-1"), None))
        .unwrap();
    let c = r.submit(user_message("t-1")).unwrap();
    assert_eq!(
        (a, b, c),
        (Sequence(0), Sequence(1), Sequence(2)),
        "no gap for the refusal"
    );
}

/// A payload-free kind emits no payload member at all, and the envelope
/// flattens: the line is one flat object keyed on kind at the top level.
///
/// **Read on `unload` rather than `load` as of 2026-08-21**, `load` having
/// stopped being payload-free when it began carrying the diagnostic
/// elections of its load. The property under test is the rendering's and
/// not that kind's, so it moves to a kind that still holds it and the run
/// bracket's other half is the nearest one.
///
/// Perturbations: remove Event.payload's skip_serializing_if and the payload
/// absence assertion fails. Separately remove Event.envelope's serde(flatten)
/// and the no-envelope-member assertion fails. Both removals were verified.
#[test]
fn bracket_kind_omits_payload_and_line_is_flat() {
    let (mut r, _path) = recorder();
    r.submit(event(Kind::Load, None, Some(elections())))
        .unwrap();
    r.submit(event(Kind::Unload, None, None)).unwrap();
    let line = r
        .structure()
        .by_kind(Kind::Unload)
        .next()
        .unwrap()
        .line
        .clone();
    assert!(
        !line.contains("\"payload\""),
        "no payload member on a bracket kind: {line}"
    );
    assert!(
        !line.contains("\"envelope\""),
        "the envelope flattens: {line}"
    );
    assert!(
        line.starts_with("{\"session\":"),
        "declaration order from the top: {line}"
    );
    assert!(
        line.contains("\"kind\":\"unload\""),
        "the dotted-name scheme's kind member: {line}"
    );
}

/// The turn close is internally tagged: one shape for both closes.
///
/// Perturbation: remove tag = "close" from TurnClose while keeping rename_all.
/// The clean close becomes a string and the expected payload assertion fails.
/// Verified with that attribute removed and restored after the failing run.
#[test]
fn turn_close_is_internally_tagged() {
    let (mut r, _path) = recorder();
    r.submit(event(Kind::Load, None, Some(elections())))
        .unwrap();
    r.submit(event(Kind::TurnStarted, Some("t-1"), None))
        .unwrap();
    r.submit(event(
        Kind::TurnClosed,
        Some("t-1"),
        Some(Payload::TurnClosed(TurnClose::Clean)),
    ))
    .unwrap();
    let line = r
        .structure()
        .by_kind(Kind::TurnClosed)
        .next()
        .unwrap()
        .line
        .clone();
    assert!(
        line.contains("\"payload\":{\"close\":\"clean\"}"),
        "internally tagged: {line}"
    );
}

/// A kind-to-payload mismatch refuses as malformed rather than rendering.
#[test]
fn mismatched_payload_refuses() {
    let (mut r, _path) = recorder();
    let err = r
        .submit(event(
            Kind::Load,
            None,
            Some(Payload::TurnClosed(TurnClose::Clean)),
        ))
        .unwrap_err();
    assert!(matches!(
        err,
        Failure::RefusedOnSubmit {
            reason: SubmitRefusal::PayloadMalformed
        }
    ));
}

/// **A measurement is a licensed kind-payload pair and splices verbatim**,
/// per Spec section 3: `model.measurement` takes `Payload::ModelMeasurement`
/// and no other kind takes that payload, and the SPU's rendered blob reaches
/// the line as the octets it arrived in, no re-encoding of this crate's
/// between them.
///
/// Perturbation: drop the `(ModelMeasurement, Some(ModelMeasurement))` arm of
/// `pairing_licensed` and the third submit refuses; widen it to
/// `(_, Some(ModelMeasurement))` and the fourth is admitted; round-trip
/// `render`'s body through `serde_json::Value` and the blob returns with its
/// members reordered. Watched under each, one at a time.
///
/// **This is the watch the retired test carried without naming.** Its
/// predecessor `absent_measurement_members_emit_nothing` asserted a skip
/// election this crate does not make, so its three assertions could not fail,
/// and the `submit` above them licensing this pair was the only construction
/// of it in the crate.
#[test]
fn the_measurement_is_a_licensed_pair_and_splices_verbatim() {
    let (mut r, _path) = recorder();
    r.submit(event(Kind::Load, None, Some(elections())))
        .unwrap();
    r.submit(event(Kind::TurnStarted, Some("t-1"), None))
        .unwrap();
    let blob = r#"{"model":"qwen3-4b-instruct","weights_hash":"sha256:abc","input_tokens":[1,2],"output_tokens":[3],"blocks":[{"label":"turn-delta","start":0,"end":2}],"timings":{"prefill_ns":"1000","decode_ns":"2000"}}"#;
    r.submit(event(
        Kind::ModelMeasurement,
        Some("t-1"),
        Some(Payload::ModelMeasurement(
            raw_payload(blob).expect("the measurement blob splices"),
        )),
    ))
    .expect("the measurement kind takes the measurement payload");
    let line = r
        .structure()
        .by_kind(Kind::ModelMeasurement)
        .next()
        .expect("the admitted event reached the structure")
        .line
        .clone();
    assert!(
        line.contains(blob),
        "the organ's octets reach the line unaltered: {line}"
    );
    let err = r
        .submit(event(
            Kind::ModelRequest,
            Some("t-1"),
            Some(Payload::ModelMeasurement(
                raw_payload(blob).expect("the measurement blob splices"),
            )),
        ))
        .unwrap_err();
    assert!(
        matches!(
            err,
            Failure::RefusedOnSubmit {
                reason: SubmitRefusal::PayloadMalformed
            }
        ),
        "no other kind takes the measurement payload"
    );
}

/// **The output carries the session's position**, per `weaver-trace-Spec`
/// section 3: an analysis placing a turn inside the context has the record
/// and nothing else once the run is over, and a member that serializes is
/// lost silently - the line still renders and every consumer still parses.
///
/// Perturbation: drop either count from `ModelOutput` and this fails.
#[test]
fn the_output_carries_the_counts() {
    let (mut r, _path) = recorder();
    r.submit(event(Kind::Load, None, Some(elections())))
        .unwrap();
    r.submit(event(Kind::TurnStarted, Some("t-1"), None))
        .unwrap();
    r.submit(event(
        Kind::ModelOutput,
        Some("t-1"),
        Some(Payload::ModelOutput(weaver_trace::ModelOutput {
            emission: "the answer".into(),
            finish: weaver_trace::Finish::Completed,
            resident: 26_214,
            capacity: 32_768,
        })),
    ))
    .unwrap();
    let line = r
        .structure()
        .by_kind(Kind::ModelOutput)
        .next()
        .unwrap()
        .line
        .clone();
    let rendered: serde_json::Value = serde_json::from_str(&line).expect("the line is one value");
    let payload = rendered
        .get("payload")
        .expect("the output carries a payload");
    assert_eq!(
        payload.get("resident").and_then(|v| v.as_u64()),
        Some(26_214),
        "the resident count reaches the record: {line}"
    );
    assert_eq!(
        payload.get("capacity").and_then(|v| v.as_u64()),
        Some(32_768),
        "the capacity reaches the record: {line}"
    );
}

/// The boundary derives its states: committed meets admitted after a drain,
/// and queued returns to zero.
#[test]
fn boundary_derives_after_drain() {
    let (mut r, _path) = recorder();
    r.submit(event(Kind::Load, None, Some(elections())))
        .unwrap();
    r.submit(event(Kind::TurnStarted, Some("t-1"), None))
        .unwrap();
    r.drain().unwrap();
    let b = r.boundary();
    assert_eq!(b.admitted, Some(Sequence(1)));
    assert_eq!(b.committed, Some(Sequence(1)));
    assert_eq!(b.queued, 0);
    assert_eq!(b.last_error, None);
}

/// An event bound to another session refuses: the recorder records one run.
#[test]
fn foreign_session_refuses() {
    let (mut r, _path) = recorder();
    let mut e = event(Kind::Load, None, Some(elections()));
    e.envelope.session = SessionRef("s-2".into());
    let err = r.submit(e).unwrap_err();
    assert!(matches!(err, Failure::RefusedOnSubmit { .. }));
}

/// A pretty-printed payload that bypasses `raw_payload` still refuses at
/// render: the two layers hold the same line, construction first and the
/// render choke point as the backstop for a `RawValue` built any other way.
///
/// Perturbation: remove the interior-newline check from `render` and the
/// stream gains extra lines from one event. Watched under exactly that
/// removal.
#[test]
fn pretty_printed_payload_refuses_at_render() {
    let (mut r, _path) = recorder();
    r.submit(event(Kind::Load, None, Some(elections())))
        .unwrap();
    let pretty = "{\n  \"role\": \"user\",\n  \"content\": []\n}";
    let bypassed = serde_json::value::RawValue::from_string(pretty.to_string())
        .expect("valid JSON, construction alone admits it");
    let err = r
        .submit(event(
            Kind::MessageUser,
            Some("t-1"),
            Some(Payload::Message(bypassed)),
        ))
        .unwrap_err();
    assert!(matches!(
        err,
        Failure::RefusedOnSubmit {
            reason: SubmitRefusal::PayloadMalformed
        }
    ));
    assert_eq!(r.structure().len(), 1, "the refused event landed nowhere");
}

/// **`unload` has two licensed pairings and no third**, per Spec section 3
/// as of 2026-09-04: payload-free where no member stood, `UnloadClose` where
/// one did, and the close's shape on any other kind refuses. Perturbation:
/// drop the `(Unload, Some(Unload))` arm of the pairing and the second
/// submit refuses; widen it to `(_, Some(Unload))` and the third passes.
#[test]
fn the_unload_carries_its_close_or_nothing() {
    let (mut r, _path) = recorder();
    let close = || {
        Some(Payload::Unload(weaver_trace::UnloadClose {
            grant_surface: weaver_trace::GrantSurface::Varied,
        }))
    };
    r.submit(event(Kind::Unload, None, None))
        .expect("payload-free where no member stood");
    r.submit(event(Kind::Unload, None, close()))
        .expect("the close where one did");
    let err = r
        .submit(event(Kind::SessionClosed, None, close()))
        .unwrap_err();
    assert!(matches!(
        err,
        Failure::RefusedOnSubmit {
            reason: SubmitRefusal::PayloadMalformed
        }
    ));
    let rendered = serde_json::to_value(weaver_trace::UnloadClose {
        grant_surface: weaver_trace::GrantSurface::Unreadable,
    })
    .expect("renders");
    assert_eq!(rendered, serde_json::json!({"grant_surface": "unreadable"}));
}

/// A run-level kind carrying a turn refuses: a join key the work never held
/// would be a false attribution. `fault` stays exempt, its option being the
/// caller's fact.
#[test]
fn turn_on_run_level_kind_refuses() {
    let (mut r, _path) = recorder();
    for kind in [Kind::Load, Kind::Unload, Kind::SessionClosed] {
        let err = r.submit(event(kind, Some("t-1"), None)).unwrap_err();
        assert!(
            matches!(
                err,
                Failure::RefusedOnSubmit {
                    reason: SubmitRefusal::PayloadMalformed
                }
            ),
            "{kind:?} with a turn must refuse"
        );
    }
    assert_eq!(r.structure().len(), 0);
    r.submit(event(Kind::Load, None, Some(elections())))
        .unwrap();
    r.submit(event(Kind::TurnStarted, Some("t-1"), None))
        .unwrap();
    let fault = raw_payload("{\"kind\":\"stub\"}").unwrap();
    r.submit(event(Kind::Fault, Some("t-1"), Some(Payload::Fault(fault))))
        .unwrap();
    let fault2 = raw_payload("{\"kind\":\"stub\"}").unwrap();
    r.submit(event(Kind::Fault, None, Some(Payload::Fault(fault2))))
        .unwrap();
}

/// A failed write is terminal and named: committed never advances past the
/// first failed sequence, later queued records are discarded with the
/// accounting kept consistent, and drain returns `CommitFailed` identifying
/// the record.
#[test]
#[cfg(target_os = "linux")]
fn failed_write_is_terminal_and_named() {
    let file = File::create("/dev/full").expect("dev full");
    let mut r = Recorder::receive(
        OwnedFd::from(file),
        RunRef("r-1".into()),
        SessionRef("s-1".into()),
    )
    .expect("receives");
    r.submit(event(Kind::Load, None, Some(elections())))
        .unwrap();
    let err = r.drain().unwrap_err();
    match err {
        Failure::CommitFailed { sequence, .. } => assert_eq!(sequence, Sequence(0)),
        other => panic!("drain names the failed record, got {other:?}"),
    }
    let b = r.boundary();
    assert_eq!(
        b.committed, None,
        "committed never advances past the failure"
    );
    assert_eq!(b.queued, 0, "accounting stays consistent");
    let next = r
        .submit(event(Kind::TurnStarted, Some("t-1"), None))
        .unwrap_err();
    assert!(matches!(
        next,
        Failure::CommitFailed {
            sequence: Sequence(0),
            ..
        }
    ));
}

/// Pressure is a reading on a recorded event and never a failure of one, per
/// `weaver-trace-Spec` section 9 as of 2026-08-22. The test drives a real
/// pipe past the mark, confirms every submission landed in the structure,
/// reads the depth from the recorder at the crossing, then kills the sink and
/// confirms the terminal-failure discard keeps the accounting consistent. The
/// absolute-full case blocks by design and is not driven here, the block
/// being backpressure a test cannot observe ending.
///
/// **This test asserted the property before the shape carried it.** It read
/// the depth off `Err(Failure::CommitPressure)` while asserting in the same
/// breath that "the report is not a refusal", which is the contradiction the
/// shape now resolves: a submission that landed answers `Ok`.
#[test]
fn high_water_reports_on_recorded_events() {
    use weaver_trace::HIGH_WATER_MARK;
    let (reader, pipe_writer) = std::io::pipe().expect("pipe");
    let mut r = Recorder::receive(
        OwnedFd::from(pipe_writer),
        RunRef("r-1".into()),
        SessionRef("s-1".into()),
    )
    .expect("receives");
    r.submit(event(Kind::Load, None, Some(elections())))
        .unwrap();
    let mut submitted = 1usize;
    let mut reported = 0usize;
    let mut depth_at_first_report = None;
    while reported == 0 && submitted < 4 * HIGH_WATER_MARK {
        match r.submit(event(
            Kind::Fault,
            None,
            Some(Payload::Fault(raw_payload("{\"kind\":\"stub\"}").unwrap())),
        )) {
            Ok(_) => submitted += 1,
            Err(other) => panic!("a submission that lands answers Ok: {other:?}"),
        }
        // The reading is taken after the submission, which is where the
        // harness takes it: the depth is the recorder's own and not the
        // submission's answer.
        let pressure = r.pressure();
        if pressure.over_mark {
            reported += 1;
            depth_at_first_report = Some(pressure.queued);
        }
    }
    assert!(reported > 0, "the mark was crossed and reported");
    assert!(
        depth_at_first_report.unwrap() > HIGH_WATER_MARK,
        "the report carries the depth that crossed the mark"
    );
    assert_eq!(
        r.structure().len(),
        submitted,
        "every reported submission landed: the report is not a refusal"
    );
    drop(reader);
    let _ = r.drain();
    assert_eq!(
        r.boundary().queued,
        0,
        "discard keeps the accounting consistent"
    );
}

/// **The subsystem's wire spellings are pinned, every case, and the organ and
/// its engine are distinct values.** The field is the record's attribution and
/// a consumer keys on the strings, so a variant rename or a serde-scheme
/// change that moved one silently would re-attribute history. The
/// organ-against-engine pair carries the claim of the #103 ruling: `spu` and
/// `spu_decoder` are two producing parties, the organ's residency facts and
/// the decode engine's model events, and a set that collapsed them would lose
/// the fact a reader of a model event wants first.
///
/// Perturbation: rename `SpuDecoder` to `Decoder`, or drop it, and this fails
/// naming the spelling. Watched by the pair below going through the same
/// serializer the recorder uses.
#[test]
fn the_subsystem_spellings_are_pinned_and_the_engine_is_not_the_organ() {
    let spelled = |subsystem: Subsystem| {
        let mut e = envelope(Kind::Load, None);
        e.subsystem = subsystem;
        serde_json::to_string(&e).expect("the envelope renders")
    };
    for (case, wire) in [
        (Subsystem::Admin, "\"subsystem\":\"admin\""),
        (Subsystem::Harness, "\"subsystem\":\"harness\""),
        (Subsystem::Spu, "\"subsystem\":\"spu\""),
        (Subsystem::SpuDecoder, "\"subsystem\":\"spu_decoder\""),
        (Subsystem::Gate, "\"subsystem\":\"gate\""),
        (Subsystem::Tool, "\"subsystem\":\"tool\""),
    ] {
        let line = spelled(case);
        assert!(line.contains(wire), "expected {wire} in {line}");
    }
    assert_ne!(
        spelled(Subsystem::Spu),
        spelled(Subsystem::SpuDecoder),
        "the organ and its engine are two attributions, which is the split's point"
    );
}

/// The classify pair admits with a turn and without one, per the charter's
/// adding text - a classify between turns belongs to no turn - and the
/// pairing is enforced: each kind takes exactly its own payload, and a
/// scored outcome under the request kind refuses.
#[test]
fn the_classify_pair_is_turn_optional_and_pairing_enforced() {
    let (mut r, _path) = recorder();
    r.submit(event(Kind::Load, None, Some(elections())))
        .unwrap();
    let ask = || {
        Some(Payload::ClassifyRequest(weaver_trace::ClassifyAsk {
            content: "the recalled passage".into(),
        }))
    };
    let outcome = || {
        Some(Payload::ClassifyOutput(weaver_trace::ClassifyScored {
            labels: vec![("entailment".into(), 0.9), ("not_entailment".into(), 0.1)],
        }))
    };
    r.submit(event(Kind::ClassifyRequest, None, ask()))
        .expect("between turns, no turn");
    r.submit(event(Kind::ClassifyOutput, None, outcome()))
        .expect("the outcome follows");
    r.submit(event(Kind::ClassifyRequest, Some("t-1"), ask()))
        .expect("within a turn, the key rides");
    // **A refused classify authors no output at all**, per the charter's
    // clause of 2026-08-22: the refusal reaches the record under its own
    // kind and `classify.output` carries the scored labels alone.
    r.submit(event(
        Kind::Refusal,
        Some("t-1"),
        Some(Payload::Refusal(
            raw_payload("{\"seam\":\"classify\",\"refusal\":{\"refusal\":\"oversized\"}}").unwrap(),
        )),
    ))
    .expect("a refusal is the record's own fact, under the class's kind");
    assert!(
        r.submit(event(Kind::ClassifyRequest, None, outcome()))
            .is_err(),
        "the pairing is total"
    );
    assert!(
        r.submit(event(Kind::ClassifyOutput, None, None)).is_err(),
        "the outcome carries its account"
    );
}

/// The system kind is real since its act, and turn-optional since the
/// prefix act: it admits under the message payload inside a turn, admits
/// with no turn because the seated identity prefix belongs to none, and the
/// wire spelling is the charter's dotted name.
///
/// **The asymmetry is the point and is asserted here rather than assumed.**
/// This test read `turn-required like its siblings` until the prefix act,
/// which moved this one kind and left the other three where they were, so
/// what it watches now is that the move was to one kind and not to the
/// message kinds as a class.
#[test]
fn the_system_kind_is_turn_optional_and_its_siblings_are_not() {
    let (mut r, path) = recorder();
    r.submit(event(Kind::Load, None, Some(elections())))
        .unwrap();
    let seq = r
        .submit(event(
            Kind::MessageSystem,
            Some("t-1"),
            Some(Payload::Message(
                raw_payload("{\"role\":\"system\",\"content\":[]}").unwrap(),
            )),
        ))
        .expect("admits inside a turn");
    assert!(seq.0 > 0);
    r.submit(event(
        Kind::MessageSystem,
        None,
        Some(Payload::Message(
            raw_payload("{\"role\":\"system\",\"content\":[]}").unwrap(),
        )),
    ))
    .expect("and admits with no turn, the seated prefix belonging to none");
    for sibling in [
        Kind::MessageUser,
        Kind::MessageAssistant,
        Kind::MessageToolResult,
    ] {
        assert!(
            r.submit(event(
                sibling,
                None,
                Some(Payload::Message(raw_payload("{}").unwrap())),
            ))
            .is_err(),
            "the other message kinds are turn-required as they were"
        );
    }
    r.drain().unwrap();
    let mut out = String::new();
    File::open(&path).unwrap().read_to_string(&mut out).unwrap();
    assert!(out.contains("\"message.system\""), "the dotted spelling");
}

/// **The `load` event carries the tee's election, written whole**, per
/// `weaver-trace-Spec` section 3
/// (`trace-load-carries-the-tee-election`): a payload rendered under a
/// non-default election carries the rule back off the canonical form, in
/// the shape the tee applied it in, so a replay stops guessing which
/// projection built the state. Perturbation: drop the member from the
/// shape and this fails.
#[test]
fn the_load_carries_the_tee_election() {
    let (mut r, path) = recorder();
    r.submit(event(
        Kind::Load,
        None,
        Some(Payload::Elections(weaver_trace::Elections {
            residual_readout: false,
            field: None,
            surprisal: false,
            tee: Some(weaver_trace::Election {
                all_kinds: false,
                keys: vec![weaver_trace::ElectedKind {
                    kind: "turn.closed".into(),
                    paths: vec!["close".into()],
                }],
            }),
            state_member: false,
            declaration: Default::default(),
            lineage: None,
            stack: Default::default(),
            state_store: Default::default(),
            composer: weaver_trace::LoopIdentity::compiled("test"),
        })),
    ))
    .unwrap();
    r.drain().unwrap();

    let mut out = String::new();
    File::open(&path).unwrap().read_to_string(&mut out).unwrap();
    assert!(
        out.contains(concat!(
            "\"tee\":{\"all_kinds\":false,",
            "\"keys\":[{\"kind\":\"turn.closed\",\"paths\":[\"close\"]}]}"
        )),
        "{out}"
    );
}

/// **The surprisal's election is written even when declined.** Charter
/// section 3.1 names each election individually so a record's posture is
/// recoverable from the record, and this one is the first that can be
/// absent while the reading it governs is present: every record written
/// before 2026-08-21 carries the surprisal vector and no flag beside it.
/// Absent, false, and true are therefore three states, and only an
/// explicit `false` separates a declined election from a record older than
/// the election.
///
/// Perturbation: give `surprisal` a `skip_serializing_if` that drops the
/// false, and the declined case becomes indistinguishable from the old
/// record. Watched under exactly that.
#[test]
fn a_declined_surprisal_election_is_written_down() {
    let (mut r, path) = recorder();
    r.submit(event(Kind::Load, None, Some(elections())))
        .unwrap();
    r.submit(event(
        Kind::Load,
        None,
        Some(Payload::Elections(weaver_trace::Elections {
            residual_readout: false,
            field: None,
            surprisal: true,
            tee: Some(weaver_trace::Election::default()),
            state_member: false,
            declaration: Default::default(),
            lineage: None,
            stack: Default::default(),
            state_store: Default::default(),
            composer: weaver_trace::LoopIdentity::compiled("test"),
        })),
    ))
    .unwrap();
    r.drain().unwrap();

    let mut out = String::new();
    File::open(&path).unwrap().read_to_string(&mut out).unwrap();
    let lines: Vec<&str> = out.lines().collect();
    assert!(
        lines[0].contains("\"surprisal\":false"),
        "the declined election is on the wire: {}",
        lines[0]
    );
    assert!(
        lines[1].contains("\"surprisal\":true"),
        "and so is the standing one: {}",
        lines[1]
    );
    assert!(
        !lines[0].contains("\"field\""),
        "while the field's absence stays an absence, the two elections \
         shaped differently on purpose"
    );
}

/// **An elision refuses a turn rather than merely not needing one.** It is
/// asked between turns on the flush's ground, so a turn on one is a
/// malformed submission and not a posture, which is the distinction
/// `turn_forbidden` draws and `turn_required` cannot: a kind that is merely
/// turn-optional admits both.
///
/// Perturbation: move `Kind::Elision` out of `turn_forbidden` and leave it
/// turn-optional, and the turn-bearing submission below is admitted.
/// Watched under exactly that move.
#[test]
fn an_elision_refuses_a_turn() {
    let (mut r, path) = recorder();
    r.submit(event(Kind::Load, None, Some(elections())))
        .unwrap();
    let span = || {
        Some(Payload::Elision(weaver_trace::ElisionSpan {
            from: 41,
            to: 57,
            resident_before: 1237,
            resident_after: 1221,
        }))
    };
    r.submit(event(Kind::Elision, None, span()))
        .expect("a turnless elision is the ordinary case");
    assert!(
        r.submit(event(Kind::Elision, Some("t-1"), span())).is_err(),
        "an elision carrying a turn is refused rather than admitted"
    );
    r.drain().unwrap();

    let mut out = String::new();
    File::open(&path).unwrap().read_to_string(&mut out).unwrap();
    let elisions: Vec<&str> = out.lines().filter(|l| l.contains("\"elision\"")).collect();
    assert_eq!(elisions.len(), 1, "only the turnless one reached the sink");
    assert!(
        elisions[0].contains("\"from\":41") && elisions[0].contains("\"to\":57"),
        "and it carries the span it removed: {}",
        elisions[0]
    );
}

/// **A recall is turnless, pairs only with its own account, and names the
/// returned events by identity alone**, per `weaver-trace-Spec` section 3's
/// recall clause. The account renders the ask and the identities in declared
/// order with an absent turn omitted, so a reader grouping by turn reads it
/// without a second lookup and a returned event's content stays where it
/// already stands in the record. A replay's whole-session answer renders its
/// two bounds and the count after them, and a partial one carries no count.
///
/// Perturbations: take `Kind::Recall` out of `turn_forbidden` and the
/// turn-bearing submission is admitted; drop its row from `pairing_licensed`
/// and the turnless one refuses; pair it with a flush's counts in the row
/// instead and the mismatched submission is admitted. Watched under each.
#[test]
fn a_recall_is_turnless_and_names_what_answered() {
    let (mut r, path) = recorder();
    r.submit(event(Kind::Load, None, Some(elections())))
        .unwrap();
    let account = || {
        Some(Payload::Recall(weaver_trace::RecallAccount {
            ask: weaver_trace::RecallAsk {
                verb: weaver_trace::RecallVerb::Recall,
                last_turns: Some(4),
            },
            returned: vec![
                weaver_trace::RecalledIdentity {
                    run: "r-0".into(),
                    turn: None,
                    sequence: "7".into(),
                    kind: "message.system".into(),
                },
                weaver_trace::RecalledIdentity {
                    run: "r-1".into(),
                    turn: Some("t-36".into()),
                    sequence: "250".into(),
                    kind: "message.assistant".into(),
                },
            ],
            count: None,
        }))
    };
    r.submit(event(Kind::Recall, None, account()))
        .expect("a turnless recall with its account is the ordinary case");
    assert!(
        r.submit(event(Kind::Recall, Some("t-37"), account()))
            .is_err(),
        "a recall carrying a turn is refused rather than admitted"
    );
    let counts = Some(Payload::Flush(weaver_trace::FlushCounts {
        resident_before: 27196,
        resident_after: 712,
    }));
    assert!(
        r.submit(event(Kind::Recall, None, counts)).is_err(),
        "a recall carrying another kind's payload is refused"
    );
    assert!(
        r.submit(event(Kind::Recall, None, None)).is_err(),
        "and a recall carrying nothing is refused"
    );
    // A whole-session answer: its bounds and the count, after `returned`.
    r.submit(event(
        Kind::Recall,
        None,
        Some(Payload::Recall(weaver_trace::RecallAccount {
            ask: weaver_trace::RecallAsk {
                verb: weaver_trace::RecallVerb::Replay,
                last_turns: None,
            },
            returned: vec![
                weaver_trace::RecalledIdentity {
                    run: "r-1".into(),
                    turn: None,
                    sequence: "0".into(),
                    kind: "load".into(),
                },
                weaver_trace::RecalledIdentity {
                    run: "r-1".into(),
                    turn: Some("t-37".into()),
                    sequence: "262".into(),
                    kind: "turn.closed".into(),
                },
            ],
            count: Some(263),
        })),
    ))
    .expect("a replay's account is the same kind");
    r.drain().unwrap();

    let mut out = String::new();
    File::open(&path).unwrap().read_to_string(&mut out).unwrap();
    let recalls: Vec<&str> = out.lines().filter(|l| l.contains("\"recall\"")).collect();
    assert_eq!(
        recalls.len(),
        2,
        "only the two well-formed recalls reached the sink"
    );
    let line: serde_json::Value = serde_json::from_str(recalls[0]).expect("the line parses");
    assert!(line.get("turn").is_none(), "belonging to no turn");
    // Read from the line's own bytes rather than a parsed value, which
    // would sort the members and hide the declared order.
    assert!(
        recalls[0].contains(concat!(
            r#""payload":{"ask":{"verb":"recall","last_turns":4},"returned":["#,
            r#"{"run":"r-0","sequence":"7","kind":"message.system"},"#,
            r#"{"run":"r-1","turn":"t-36","sequence":"250","kind":"message.assistant"}]}"#
        )),
        "the ask and the identities, in declared order, and no pairs: {}",
        recalls[0]
    );
    assert!(
        recalls[1].contains(concat!(
            r#""payload":{"ask":{"verb":"replay"},"returned":["#,
            r#"{"run":"r-1","sequence":"0","kind":"load"},"#,
            r#"{"run":"r-1","turn":"t-37","sequence":"262","kind":"turn.closed"}],"#,
            r#""count":263}"#
        )),
        "a whole-session answer by its bounds and its count: {}",
        recalls[1]
    );
}

/// **A restored message is turnless and carries the message whole**, per
/// `weaver-trace-Spec` section 3's restored-prefix clause: a restoring
/// load's conversation is seated ahead of every turn, so the kind belongs to
/// none, and its payload is the message the harness rendered, role and
/// content, spliced as the identity's is. The four turned message kinds are
/// not moved by it, and a user message with no turn still refuses.
///
/// Perturbations: take `Kind::MessageRestored` out of `turn_forbidden` and
/// the turn-bearing submission is admitted; drop it from the message row of
/// `pairing_licensed` and the turnless one refuses. Watched under each.
#[test]
fn a_restored_message_is_turnless_and_carries_the_message_whole() {
    let (mut r, path) = recorder();
    r.submit(event(Kind::Load, None, Some(elections())))
        .unwrap();
    let message = || {
        Some(Payload::Message(
            raw_payload(r#"{"role":"user","content":[{"type":"text","text":"hello"}]}"#).unwrap(),
        ))
    };
    r.submit(event(Kind::MessageRestored, None, message()))
        .expect("a turnless restored message is the ordinary case");
    assert!(
        r.submit(event(Kind::MessageRestored, Some("t-1"), message()))
            .is_err(),
        "a restored message carrying a turn is refused rather than admitted"
    );
    assert!(
        r.submit(event(Kind::MessageRestored, None, None)).is_err(),
        "a restored message carrying nothing is refused"
    );
    let counts = Some(Payload::Flush(weaver_trace::FlushCounts {
        resident_before: 27196,
        resident_after: 712,
    }));
    assert!(
        r.submit(event(Kind::MessageRestored, None, counts))
            .is_err(),
        "and one carrying another kind's payload is refused"
    );
    assert!(
        r.submit(event(Kind::MessageUser, None, message())).is_err(),
        "the turned user kind still refuses a message with no turn"
    );
    r.drain().unwrap();

    let mut out = String::new();
    File::open(&path).unwrap().read_to_string(&mut out).unwrap();
    let restored: Vec<&str> = out
        .lines()
        .filter(|l| l.contains(r#""kind":"message.restored""#))
        .collect();
    assert_eq!(
        restored.len(),
        1,
        "only the well-formed one reached the sink"
    );
    let line: serde_json::Value = serde_json::from_str(restored[0]).expect("the line parses");
    assert!(line.get("turn").is_none(), "belonging to no turn");
    assert!(
        restored[0]
            .contains(r#""payload":{"role":"user","content":[{"type":"text","text":"hello"}]}"#),
        "the message whole, role and content: {}",
        restored[0]
    );
}

/// **A score is turnless and carries the verdict and the ratio's terms**, per
/// `weaver-trace-Spec` section 3's score clause (#523). The predicate and
/// whether it held render in declared order, the ratio as its two integer
/// terms where the task supplies a denominator, and nothing where it does not,
/// so an absent ratio is never read as a zero or a one.
///
/// Perturbations: take `Kind::Score` out of `turn_forbidden` and the turned
/// submission is admitted; drop its row from `pairing_licensed` and the
/// turnless one refuses. Watched under each.
#[test]
fn a_score_is_turnless_and_carries_the_verdict_and_its_terms() {
    let (mut r, path) = recorder();
    r.submit(event(Kind::Load, None, Some(elections())))
        .unwrap();
    let scored = |ratio: Option<weaver_trace::ScoreRatio>| {
        Some(Payload::Score(weaver_trace::TaskScore {
            predicate: "reached-the-goal".into(),
            passed: true,
            ratio,
        }))
    };
    let terms = || {
        Some(weaver_trace::ScoreRatio {
            measured: 14,
            denominator: 11,
        })
    };
    r.submit(event(Kind::Score, None, scored(terms())))
        .expect("a turnless score with its terms is the ordinary case");
    r.submit(event(Kind::Score, None, scored(None)))
        .expect("and one whose task supplies no denominator");
    assert!(
        r.submit(event(Kind::Score, Some("t-3"), scored(terms())))
            .is_err(),
        "a score carrying a turn is refused rather than admitted"
    );
    assert!(
        r.submit(event(Kind::Score, None, None)).is_err(),
        "a score carrying nothing is refused"
    );
    let counts = Some(Payload::Flush(weaver_trace::FlushCounts {
        resident_before: 27196,
        resident_after: 712,
    }));
    assert!(
        r.submit(event(Kind::Score, None, counts)).is_err(),
        "and one carrying another kind's payload is refused"
    );
    r.drain().unwrap();

    let mut out = String::new();
    File::open(&path).unwrap().read_to_string(&mut out).unwrap();
    let scores: Vec<&str> = out
        .lines()
        .filter(|l| l.contains(r#""kind":"score""#))
        .collect();
    assert_eq!(
        scores.len(),
        2,
        "only the two well-formed scores reached the sink"
    );
    let line: serde_json::Value = serde_json::from_str(scores[0]).expect("the line parses");
    assert!(line.get("turn").is_none(), "belonging to no turn");
    assert!(
        scores[0].contains(concat!(
            r#""payload":{"predicate":"reached-the-goal","passed":true,"#,
            r#""ratio":{"measured":14,"denominator":11}}"#
        )),
        "the verdict and the ratio's two terms, in declared order: {}",
        scores[0]
    );
    assert!(
        scores[1].contains(r#""payload":{"predicate":"reached-the-goal","passed":true}"#),
        "and no ratio where the task supplied no denominator: {}",
        scores[1]
    );
}

/// **A submission that lands answers `Ok` whatever the queue holds**, per
/// `weaver-trace-Spec` section 9 as of 2026-08-22, and the depth is a
/// reading taken from the recorder rather than an answer to a submission.
///
/// The property that makes this the right shape: past the mark, the event is
/// in the working structure and the answer is `Ok`. Before this act the same
/// event produced `Err`, so a caller could not act on pressure without also
/// treating a recorded event as a lost one.
///
/// Perturbation: return `Err` past the mark again and the first assertion
/// fires on a submission whose event is in the structure beside it.
#[test]
fn a_submission_past_the_mark_still_answers_ok() {
    use weaver_trace::HIGH_WATER_MARK;
    let (reader, pipe_writer) = std::io::pipe().expect("pipe");
    let mut r = Recorder::receive(
        OwnedFd::from(pipe_writer),
        RunRef("r-1".into()),
        SessionRef("s-1".into()),
    )
    .expect("receives");
    r.submit(event(Kind::Load, None, Some(elections())))
        .unwrap();

    let mut submitted = 1usize;
    while !r.pressure().over_mark && submitted < 4 * HIGH_WATER_MARK {
        r.submit(event(
            Kind::Fault,
            None,
            Some(Payload::Fault(raw_payload("{\"kind\":\"stub\"}").unwrap())),
        ))
        .expect("a submission that lands answers Ok");
        submitted += 1;
    }
    assert!(r.pressure().over_mark, "the mark was crossed");

    // The one that matters: past the mark, still Ok, and in the structure.
    let before = r.structure().len();
    r.submit(event(
        Kind::Fault,
        None,
        Some(Payload::Fault(raw_payload("{\"kind\":\"stub\"}").unwrap())),
    ))
    .expect("a submission past the mark answers Ok because its event landed");
    assert_eq!(
        r.structure().len(),
        before + 1,
        "and the event it answered for is in the structure"
    );
    assert!(
        r.pressure().queued > HIGH_WATER_MARK,
        "the depth is readable and says what it holds"
    );

    drop(reader);
    let _ = r.drain();
}

/// **The `load` event names its loop and its member**, per `weaver-trace-Spec`
/// section 3 (`trace-load-names-its-loop-and-its-member`): a payload rendered
/// with a file-backed composer and a standing member reads both back off the
/// canonical form, the digest and the path included, and a compiled loop
/// renders no file and no digest rather than empty ones.
///
/// Perturbation: skip either member's serialization and this fails, on the
/// same ground as the tee's watch: a member that serializes is easy to lose
/// and losing it is silent.
#[test]
fn the_load_names_its_loop_and_its_member() {
    let rendered = serde_json::to_value(weaver_trace::Elections {
        residual_readout: false,
        field: None,
        surprisal: false,
        tee: Some(weaver_trace::Election::default()),
        state_member: true,
        declaration: "ab".repeat(32),
        lineage: None,
        stack: Default::default(),
        state_store: weaver_trace::StoreIdentity {
            engine: "postgres".into(),
            database: Some("weaver_karl".into()),
            role: Some("weaver_karl".into()),
        },
        composer: weaver_trace::LoopIdentity::file(
            "pyworker",
            std::path::Path::new("/usr/local/libexec/weaver/loops/dev_loop.py"),
            Some("ab".repeat(32)),
        ),
    })
    .expect("renders");
    assert_eq!(rendered["state_member"], serde_json::json!(true));
    assert_eq!(
        rendered["state_store"],
        serde_json::json!({"engine": "postgres", "database": "weaver_karl", "role": "weaver_karl"}),
        "the load names the store the member stands on"
    );
    assert_eq!(
        serde_json::to_value(weaver_trace::StoreIdentity {
            engine: "sqlite".into(),
            database: None,
            role: None
        })
        .expect("renders"),
        serde_json::json!({"engine": "sqlite"}),
        "the embedded engine names no database and no role, not null ones"
    );
    assert_eq!(
        rendered["composer"]["binary"],
        serde_json::json!("pyworker")
    );
    assert_eq!(
        rendered["composer"]["file"],
        serde_json::json!("/usr/local/libexec/weaver/loops/dev_loop.py")
    );
    assert_eq!(
        rendered["composer"]["sha256"],
        serde_json::json!("ab".repeat(32))
    );

    let compiled =
        serde_json::to_value(weaver_trace::LoopIdentity::compiled("worker")).expect("renders");
    assert_eq!(
        compiled,
        serde_json::json!({"binary": "worker"}),
        "no file and no digest, not empty ones"
    );
    let absent = serde_json::to_value(weaver_trace::Elections {
        residual_readout: false,
        field: None,
        surprisal: false,
        tee: None,
        state_member: false,
        declaration: Default::default(),
        lineage: None,
        stack: Default::default(),
        state_store: Default::default(),
        composer: weaver_trace::LoopIdentity::compiled("worker"),
    })
    .expect("renders");
    assert_eq!(
        absent["state_member"],
        serde_json::json!(false),
        "false is written, not omitted"
    );
}
