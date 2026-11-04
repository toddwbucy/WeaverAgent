use super::event::admit;
use super::payloads::*;
use super::*;
use crate::{MonotonicNs, RunRef, Sequence, SessionRef, Subsystem, TurnRef};

pub(super) fn change(offer: bool) -> EmissionChangeRecord {
    let source = |sequence, block| SourceBlockId {
        run: "run".into(),
        sequence: DecimalU64(sequence),
        block: DecimalU64(block),
    };
    let span = |start, end| Span {
        start: DecimalU64(start),
        end: DecimalU64(end),
    };
    EmissionChangeRecord {
        matched: ThesaurusMatch {
            source: source(10, 2),
            previous_change: None,
            sentence_span: span(0, 9),
            word_span: span(4, 8),
            scheme_id: format!("ste100:{}", "b".repeat(64)),
            thesaurus_sha256: "a".repeat(64),
            entry_word: "coal".into(),
        },
        span: if offer { span(0, 9) } else { span(4, 8) },
        replaced: if offer { "Use coal." } else { "coal" }.into(),
        replacement: if offer { "Use fuel." } else { "fuel" }.into(),
        method: if offer {
            EmissionChangeMethod::Offer {
                instruction: source(12, 0),
                reemission: source(13, 3),
                reemission_span: span(2, 11),
            }
        } else {
            EmissionChangeMethod::Splice {}
        },
    }
}

pub(super) fn change_event(offer: bool) -> Event {
    Event {
        envelope: Envelope {
            session: SessionRef("session".into()),
            run: RunRef("run".into()),
            turn: Some(TurnRef("turn".into())),
            sequence: Sequence(999),
            kind: Kind::EmissionChange,
            channel: Channel::Internal,
            origin: Origin::Loop,
            subsystem: Subsystem::Harness,
            causal_parent: Some(Sequence(if offer { 13 } else { 10 })),
            wall_ms: 42,
            monotonic_ns: MonotonicNs(100),
        },
        payload: Some(Payload::EmissionChange(Box::new(change(offer)))),
    }
}

fn accepts(event: &Event) -> bool {
    admit(event, &SessionRef("session".into()), &RunRef("run".into())).is_ok()
}

#[test]
fn emission_change_has_exact_canonical_fields_and_no_selectable_projection() {
    for (offer, fixture) in [
        (
            false,
            include_str!("../../tests/fixtures/ste-emission-splice.json"),
        ),
        (
            true,
            include_str!("../../tests/fixtures/ste-emission-offer.json"),
        ),
    ] {
        assert_eq!(
            serde_json::to_string(&change(offer)).unwrap(),
            fixture.trim()
        );
    }
    assert_eq!(Kind::EmissionChange.as_str(), "emission.change");
    assert_eq!(content_schema(Kind::EmissionChange), ContentSchema::None);
    assert_eq!(turn_rule(Kind::EmissionChange), TurnRule::Required);
}

#[test]
fn emission_change_licenses_only_harness_loop_turns_and_its_exact_parent() {
    for offer in [false, true] {
        let good = change_event(offer);
        assert!(accepts(&good));
        for index in 0..7 {
            let mut wrong = good.clone();
            match index {
                0 => wrong.envelope.channel = Channel::Output,
                1 => wrong.envelope.origin = Origin::Model,
                2 => wrong.envelope.subsystem = Subsystem::Spu,
                3 => wrong.envelope.turn = None,
                4 => wrong.envelope.causal_parent = None,
                5 => wrong.envelope.causal_parent = Some(Sequence(8)),
                6 => wrong.envelope.kind = Kind::Fault,
                _ => unreachable!(),
            }
            assert!(!accepts(&wrong), "offer={offer}, field={index}");
        }
        let mut next = good;
        let Some(Payload::EmissionChange(record)) = &mut next.payload else {
            unreachable!()
        };
        record.matched.previous_change = Some(11.into());
        if !offer {
            assert!(!accepts(&next));
            next.envelope.causal_parent = Some(Sequence(11));
        }
        assert!(accepts(&next));
    }
}

#[test]
fn emission_change_escapes_text_without_changing_decoded_bytes() {
    let mut event = change_event(true);
    let Some(Payload::EmissionChange(record)) = &mut event.payload else {
        unreachable!()
    };
    record.replaced = "Read \"caf\u{e9}\".\n".into();
    record.replacement = "Use \"fuel\".\n".into();
    let bytes = serde_json::to_string(&event).unwrap();
    assert!(!bytes.contains(['\n', '\r']));
    let decoded: serde_json::Value = serde_json::from_str(&bytes).unwrap();
    assert_eq!(decoded["payload"]["replaced"], "Read \"caf\u{e9}\".\n");
    assert_eq!(decoded["payload"]["replacement"], "Use \"fuel\".\n");
}
