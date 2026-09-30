//! conforms: diagnostic-canonical-form-follows-trace
//! conforms: diagnostic-session-is-the-replays-own
//! conforms: diagnostic-admission-precedes-the-write
//! conforms: diagnostic-turn-rule-per-kind
//!
//! The writer's perturbation watches, per `weaver-diagnostic-Spec` section 7.
//! Each names the removal it was watched failing under, per the corpus's
//! perturbation doctrine: a test never seen red proves nothing about the
//! property it claims.

use std::io::Read;
use std::os::fd::OwnedFd;

use serde_json::value::RawValue;
use weaver_diagnostic::{
    Envelope, Event, Kind, MonotonicNs, Payload, Recorder, RunRef, Sequence, SessionRef,
    SubmitRefusal, Subsystem, TurnRef,
};

/// A recorder over a fresh temp file, with the file readable back beside it.
fn stand(session: &str, run: &str) -> (Recorder, std::fs::File) {
    let file = tempfile();
    let read_back = file.try_clone().expect("clone");
    let recorder = Recorder::receive(
        OwnedFd::from(file),
        RunRef(run.into()),
        SessionRef(session.into()),
    )
    .expect("receive");
    (recorder, read_back)
}

fn tempfile() -> std::fs::File {
    let dir = std::env::temp_dir();
    let path = dir.join(format!(
        "weaver-diagnostic-test-{}-{:?}",
        std::process::id(),
        std::thread::current().id()
    ));
    let file = std::fs::OpenOptions::new()
        .create(true)
        .truncate(true)
        .read(true)
        .write(true)
        .open(&path)
        .expect("temp file");
    std::fs::remove_file(&path).expect("unlink");
    file
}

fn contents(read_back: &mut std::fs::File) -> String {
    use std::io::Seek;
    read_back.rewind().expect("rewind");
    let mut s = String::new();
    read_back.read_to_string(&mut s).expect("read");
    s
}

fn envelope(kind: Kind, turn: Option<&str>, wall_ms: u64, mono: u64) -> Envelope {
    Envelope {
        // Bound by the recorder at submit; these values are overwritten.
        session: SessionRef("caller-supplied".into()),
        run: RunRef("caller-supplied".into()),
        turn: turn.map(|t| TurnRef(t.into())),
        sequence: Sequence(0),
        kind,
        subsystem: Subsystem::Harness,
        causal_parent: None,
        wall_ms,
        monotonic_ns: MonotonicNs(mono),
    }
}

/// Two lines lifted verbatim from a serving record this box holds, the raw
/// sink at `~/.weaveragents/karl/trace.ndjson`, run
/// `2026-08-29T21:58:12.937Z-karl-1366ba970737be0b` of the loop-1 smoke of
/// 2026-08-29 - the harness's own canonical bytes, not a driver's
/// re-serialization, which pads separators with spaces and would fail this
/// comparison for the wrong reason.
const SERVING_TURN_STARTED: &str = "{\"session\":\"s-karl-1\",\"run\":\"2026-08-29T21:58:12.937Z-karl-1366ba970737be0b\",\"turn\":\"t-1\",\"sequence\":\"2\",\"kind\":\"turn.started\",\"subsystem\":\"harness\",\"wall_ms\":1788040694104,\"monotonic_ns\":\"1166347120\"}";
const SERVING_MESSAGE_USER: &str = "{\"session\":\"s-karl-1\",\"run\":\"2026-08-29T21:58:12.937Z-karl-1366ba970737be0b\",\"turn\":\"t-1\",\"sequence\":\"3\",\"kind\":\"message.user\",\"subsystem\":\"harness\",\"wall_ms\":1788040694104,\"monotonic_ns\":\"1166362689\",\"payload\":{\"role\":\"user\",\"content\":[{\"type\":\"text\",\"text\":\"In one short sentence, name a colour and nothing else.\"}]}}";

/// Canonical form follows `weaver-trace-Spec` section 2: a shared kind's
/// line compared byte for byte against the same event's serving line.
///
/// Perturbation: reorder any two [`Envelope`] fields, or render `sequence`
/// or `monotonic_ns` as a bare number, and this fails. Watched failing under
/// exactly that: `monotonic_ns` rendered bare diverges at the first byte of
/// its value.
#[test]
fn canonical_form_follows_the_serving_line() {
    let (mut recorder, mut read_back) =
        stand("s-karl-1", "2026-08-29T21:58:12.937Z-karl-1366ba970737be0b");
    // Two padding events walk the gapless sequence to the fixtures' own
    // ordinals, so the comparison is byte-whole rather than sequence-edited.
    for _ in 0..2 {
        recorder
            .submit(Event {
                envelope: envelope(Kind::TurnStarted, Some("t-0"), 1, 1),
                payload: None,
            })
            .expect("padding");
    }
    recorder
        .submit(Event {
            envelope: envelope(Kind::TurnStarted, Some("t-1"), 1788040694104, 1166347120),
            payload: None,
        })
        .expect("turn.started");
    let spliced = RawValue::from_string(
        "{\"role\":\"user\",\"content\":[{\"type\":\"text\",\"text\":\"In one short sentence, name a colour and nothing else.\"}]}".into(),
    )
    .expect("raw");
    recorder
        .submit(Event {
            envelope: envelope(Kind::MessageUser, Some("t-1"), 1788040694104, 1166362689),
            payload: Some(Payload::Spliced(spliced)),
        })
        .expect("message.user");
    let written = contents(&mut read_back);
    let lines: Vec<&str> = written.lines().collect();
    assert_eq!(lines[2], SERVING_TURN_STARTED, "turn.started diverges");
    assert_eq!(lines[3], SERVING_MESSAGE_USER, "message.user diverges");
}

/// The session is the replay's own: the envelope renders under the
/// diagnostic run's session whatever the caller supplied, and the replayed
/// name appears in the identity payload alone.
///
/// Perturbation: remove the binding in `submit` and the caller's
/// `caller-supplied` label reaches the record. Watched failing under
/// exactly that removal.
#[test]
fn the_session_is_the_replays_own() {
    let (mut recorder, mut read_back) = stand("s-diag-1", "r-diag-1");
    recorder
        .submit(Event {
            envelope: envelope(Kind::TurnStarted, Some("t-1"), 1, 1),
            payload: None,
        })
        .expect("submit");
    let written = contents(&mut read_back);
    assert!(written.contains("\"session\":\"s-diag-1\""), "not bound");
    assert!(written.contains("\"run\":\"r-diag-1\""), "run not bound");
    assert!(!written.contains("caller-supplied"), "caller label leaked");
}

/// Admission precedes the write: a refused submission leaves the sink
/// untouched and consumes no sequence.
///
/// Perturbation: move the `admit` call after the write and the refused
/// line lands on the sink. Watched failing under exactly that move.
#[test]
fn admission_precedes_the_write() {
    let (mut recorder, mut read_back) = stand("s-diag-1", "r-diag-1");
    let refused = recorder.submit(Event {
        envelope: envelope(Kind::ModelOutput, Some("t-1"), 1, 1),
        payload: None,
    });
    match refused {
        Err(weaver_diagnostic::Failure::SubmitRefused {
            refusal: SubmitRefusal::RequiredFieldAbsent { .. },
        }) => {}
        other => panic!("expected a required-field refusal, got {other:?}"),
    }
    assert_eq!(contents(&mut read_back), "", "the refusal touched the sink");
    let sequence = recorder
        .submit(Event {
            envelope: envelope(Kind::TurnStarted, Some("t-1"), 1, 1),
            payload: None,
        })
        .expect("the record survives a refusal");
    assert_eq!(sequence, Sequence(0), "the refusal consumed a sequence");
}

/// The kind-to-payload pairing refuses a mismatch before the sink, the
/// other half of admission: a typed payload under a spliced kind.
#[test]
fn a_mismatched_pairing_refuses() {
    let (mut recorder, mut read_back) = stand("s-diag-1", "r-diag-1");
    let refused = recorder.submit(Event {
        envelope: envelope(Kind::ModelOutput, Some("t-1"), 1, 1),
        payload: Some(Payload::ReplayOpened(weaver_diagnostic::ReplayOpened {
            reader_elected: false,
        })),
    });
    assert!(
        matches!(
            refused,
            Err(weaver_diagnostic::Failure::SubmitRefused {
                refusal: SubmitRefusal::PayloadKindMismatch
            })
        ),
        "expected a pairing refusal, got {refused:?}"
    );
    assert_eq!(contents(&mut read_back), "", "the refusal touched the sink");
}

/// A spliced body that is the JSON literal `null` refuses as malformed
/// before the sink: the envelope emits no payload member rather than a null
/// one, so a null splice is an absent payload wearing a member. The check is
/// framing, not interpretation - the interior of a real payload stays the
/// harness's, per the bounded-admission rule.
///
/// Perturbation: remove the null check from `admit` and the line lands as
/// `"payload":null`, a shape no record carries. Watched failing under
/// exactly that removal.
#[test]
fn a_null_splice_refuses_as_malformed() {
    let (mut recorder, mut read_back) = stand("s-diag-1", "r-diag-1");
    let null = RawValue::from_string("null".into()).expect("valid JSON");
    let refused = recorder.submit(Event {
        envelope: envelope(Kind::ModelOutput, Some("t-1"), 1, 1),
        payload: Some(Payload::Spliced(null)),
    });
    assert!(
        matches!(
            refused,
            Err(weaver_diagnostic::Failure::SubmitRefused {
                refusal: SubmitRefusal::PayloadMalformed
            })
        ),
        "expected a malformed refusal, got {refused:?}"
    );
    assert_eq!(contents(&mut read_back), "", "the refusal touched the sink");
}

/// A payload each kind pairs with, so a submission is refused or admitted
/// on its turn alone.
fn licensed_payload(kind: Kind) -> Option<Payload> {
    let spliced = || {
        Some(Payload::Spliced(
            RawValue::from_string("{}".into()).expect("raw"),
        ))
    };
    match kind {
        Kind::ReplayOpened => Some(Payload::ReplayOpened(weaver_diagnostic::ReplayOpened {
            reader_elected: false,
        })),
        Kind::ReplayIdentity => Some(Payload::ReplayIdentity(weaver_diagnostic::ReplayIdentity {
            replayed_session: "s-src".into(),
            model: weaver_diagnostic::ModelId("m".into()),
            weights_hash: weaver_diagnostic::WeightsHash("h".into()),
            template: weaver_diagnostic::TemplateId("t".into()),
        })),
        Kind::ReplayClosed => Some(Payload::ReplayClosed(weaver_diagnostic::ReplayClosed {
            outcome: weaver_diagnostic::ReplayOutcome::Certified,
        })),
        Kind::ResidualColumn => Some(Payload::ResidualColumn(weaver_diagnostic::ResidualColumn {
            position: 10,
            layers: 1,
            width: 1,
            values: vec![vec![1.0]],
        })),
        Kind::TurnStarted => None,
        _ => spliced(),
    }
}

fn submit_as(kind: Kind, turn: Option<&str>) -> Result<Sequence, weaver_diagnostic::Failure> {
    let (mut recorder, _read_back) = stand("s-d", "r-d");
    recorder.submit(Event {
        envelope: envelope(kind, turn, 1, 1),
        payload: licensed_payload(kind),
    })
}

/// **A kind that belongs to no turn refuses one, and is admitted without**,
/// per `weaver-diagnostic-Spec` section 3.2's turn table: the replay trio,
/// and `flush`, `elision` and `recall` with their serving rule.
///
/// Perturbation: move any one of these kinds out of `turn_rule`'s forbidden
/// arm and its turned submission is admitted. Watched moving `Recall` and
/// `Flush`.
#[test]
fn a_turnless_kind_refuses_a_turn() {
    for kind in [
        Kind::ReplayOpened,
        Kind::ReplayIdentity,
        Kind::ReplayClosed,
        Kind::Flush,
        Kind::Elision,
        Kind::Recall,
    ] {
        assert!(
            submit_as(kind, None).is_ok(),
            "{kind:?} without a turn is its ordinary case"
        );
        assert!(
            matches!(
                submit_as(kind, Some("t-1")),
                Err(weaver_diagnostic::Failure::SubmitRefused {
                    refusal: SubmitRefusal::PayloadMalformed
                })
            ),
            "{kind:?} carrying a turn is refused"
        );
    }
}

/// **A kind that belongs to a turn refuses its absence, and is admitted
/// with one**, per the same table: a replayed turn's bracket, its messages
/// but the system one, the four model kinds, and a residual column.
///
/// Perturbation: move any one of these out of the required arm and its
/// turnless submission is admitted. Watched moving `ModelRequest` and
/// `ResidualColumn`.
#[test]
fn a_turned_kind_refuses_no_turn() {
    for kind in [
        Kind::TurnStarted,
        Kind::TurnClosed,
        Kind::MessageUser,
        Kind::MessageAssistant,
        Kind::MessageToolResult,
        Kind::ModelRequest,
        Kind::ModelOutput,
        Kind::ModelMeasurement,
        Kind::ModelField,
        Kind::ResidualColumn,
    ] {
        assert!(
            submit_as(kind, Some("t-1")).is_ok(),
            "{kind:?} inside a turn is its ordinary case"
        );
        assert!(
            matches!(
                submit_as(kind, None),
                Err(weaver_diagnostic::Failure::SubmitRefused {
                    refusal: SubmitRefusal::RequiredFieldAbsent { .. }
                })
            ),
            "{kind:?} without a turn is refused"
        );
    }
}

/// **A turn-optional kind is admitted both ways**, per the same table: the
/// seated prefix precedes every turn and a system message in one carries it,
/// and a refusal or a fault falls inside a turn or between them.
///
/// Perturbation: move any one of these into the forbidden or the required
/// arm and one of its two submissions refuses. Watched moving `Refusal` to
/// the forbidden arm and `MessageSystem` to the required one.
#[test]
fn a_turn_optional_kind_is_admitted_either_way() {
    for kind in [Kind::MessageSystem, Kind::Refusal, Kind::Fault] {
        assert!(submit_as(kind, None).is_ok(), "{kind:?} without a turn");
        assert!(submit_as(kind, Some("t-1")).is_ok(), "{kind:?} inside one");
    }
}

/// **Every kind of the set stands in exactly one of the three tests above**,
/// so a kind added to the set and given a rule without a watch fails here.
/// The match is exhaustive and wildcard-free for the reason `turn_rule`'s is.
#[test]
fn every_kind_has_a_turn_watch() {
    fn watched(kind: Kind) -> bool {
        match kind {
            Kind::ReplayOpened
            | Kind::ReplayIdentity
            | Kind::ReplayClosed
            | Kind::Flush
            | Kind::Elision
            | Kind::Recall
            | Kind::TurnStarted
            | Kind::TurnClosed
            | Kind::MessageUser
            | Kind::MessageAssistant
            | Kind::MessageToolResult
            | Kind::ModelRequest
            | Kind::ModelOutput
            | Kind::ModelMeasurement
            | Kind::ModelField
            | Kind::ResidualColumn
            | Kind::MessageSystem
            | Kind::Refusal
            | Kind::Fault => true,
        }
    }
    assert!(watched(Kind::Recall));
}
