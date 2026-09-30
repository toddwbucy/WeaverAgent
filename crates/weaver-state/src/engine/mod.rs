//! conforms: state-store-is-a-port
//!
//! The engines behind the port, one module each behind its feature, per
//! `weaver-state-Spec` sections 1 and 3.

#[cfg(feature = "postgres")]
pub mod postgres;
#[cfg(feature = "sqlite")]
pub mod sqlite;

#[cfg(test)]
fn raw_objects_survive_the_engine_and_answers(store: &mut dyn crate::store::Store) {
    use crate::store::{
        Distillate, render_identity_answer, render_recall_answer, render_replay_answer,
    };
    use serde_json::value::RawValue;
    use std::collections::BTreeMap;

    const RAW: &str = r#"{"z": 1.00, "a": {"second":2,"first":1}}"#;
    store
        .land(&Distillate {
            session: "raw-session".into(),
            run: "raw-run".into(),
            turn: None,
            kind: "message.system".into(),
            sequence: 1,
            pairs: vec![("content".into(), RAW.into())],
        })
        .expect("lands raw object");
    for (ask, field, events, render) in [
        (
            "replay",
            "events",
            store.replay("raw-session").unwrap(),
            render_replay_answer as fn(&[crate::store::RecalledEvent]) -> String,
        ),
        (
            "recall",
            "events",
            store.recall("raw-session", None).unwrap(),
            render_recall_answer,
        ),
        (
            "identity",
            "messages",
            store.identity("raw-session").unwrap(),
            render_identity_answer,
        ),
    ] {
        assert_eq!(events.len(), 1, "{ask}");
        assert_eq!(
            events[0].pairs,
            [("content".into(), RAW.into())],
            "{ask} engine bytes"
        );
        let object =
            |raw: &str| serde_json::from_str::<BTreeMap<String, Box<RawValue>>>(raw).unwrap();
        let frame = object(&render(&events));
        let answer = object(frame["answer"].get());
        let body = object(answer[ask].get());
        let rows: Vec<Box<RawValue>> = serde_json::from_str(body[field].get()).unwrap();
        let row = object(rows[0].get());
        let pairs = object(row["pairs"].get());
        assert_eq!(pairs["content"].get(), RAW, "{ask} answer bytes");
    }
}

/// Four lines of a recorded trace, verbatim: karl's seated prefix, a user
/// message, an assistant message and a measurement, from the olympus baseline
/// of 2026-09-27 on the shared bulk store.
#[cfg(test)]
const RECORDED: &str = include_str!("recorded-karl.ndjson");

/// **A recorded line of each typed kind projects through the tee, lands, and
/// serves the value a direct read of the record gives**, per
/// `weaver-state-Spec` section 3's typed landing. Each line is distilled by
/// the tee the harness applies, under an election naming the typed members
/// and one member that is not, parsed as the ingest parses it and landed; the
/// replay then serves each event's pairs byte for byte as they crossed, the
/// message's served members read through the floor's type equal the record's
/// payload read the same way, and the measurement's readings equal the
/// record's. A second measurement crosses with its perplexity absent and is
/// served with no perplexity. Each engine's caller then reads its own tables
/// for the rows that prove the landing was typed.
#[cfg(test)]
fn recorded_lines_land_typed_and_serve_what_the_record_reads(store: &mut dyn crate::store::Store) {
    use crate::store::parse_distillate;
    use std::collections::BTreeMap;
    use weaver_trace::{ElectedKind, Election, distill};

    let elected = |kind: &str, paths: &[&str]| ElectedKind {
        kind: kind.into(),
        paths: paths.iter().map(|p| p.to_string()).collect(),
    };
    let election = Election {
        all_kinds: false,
        keys: vec![
            elected("message.user", &["role", "content"]),
            elected("message.assistant", &["role", "content"]),
            elected(
                "model.measurement",
                &["perplexity", "entropies", "surprisals", "input_tokens"],
            ),
        ],
    };
    let mut crossed = Vec::new();
    for line in RECORDED.lines() {
        let frame = distill(line, &election).expect("every recorded kind is elected");
        let distillate = parse_distillate(&frame).expect("the tee's frame parses");
        store.land(&distillate).expect("lands");
        crossed.push(distillate);
    }
    // The second measurement: the recorded one with its perplexity absent,
    // which the SPU renders as no member at all.
    let mut absent = crossed[3].clone();
    absent.sequence += 1;
    absent.pairs.retain(|(key, _)| key != "perplexity");
    store.land(&absent).expect("lands");
    crossed.push(absent);

    let served = store.replay("s-karl-1").expect("replays");
    assert_eq!(served.len(), crossed.len());
    for ((line, sent), event) in RECORDED
        .lines()
        .map(Some)
        .chain([None])
        .zip(&crossed)
        .zip(&served)
    {
        assert_eq!(
            event.pairs, sent.pairs,
            "{} served as it crossed",
            sent.kind
        );
        let pairs: BTreeMap<&str, &str> = event
            .pairs
            .iter()
            .map(|(k, v)| (k.as_str(), v.as_str()))
            .collect();
        let Some(line) = line else {
            assert!(
                !pairs.contains_key("perplexity"),
                "an absent perplexity stays absent"
            );
            assert!(pairs.contains_key("entropies"));
            continue;
        };
        let record: serde_json::Value = serde_json::from_str(line).expect("recorded line");
        let payload = &record["payload"];
        if sent.kind.starts_with("message.") {
            let direct: weaver_traits::Message =
                serde_json::from_value(payload.clone()).expect("the record's message");
            let through = weaver_traits::Message {
                role: serde_json::from_str(pairs["role"]).expect("served role"),
                content: serde_json::from_str(pairs["content"]).expect("served content"),
            };
            assert_eq!(through, direct, "{}", sent.kind);
        } else {
            let perplexity: f64 = serde_json::from_str(pairs["perplexity"]).unwrap();
            assert_eq!(Some(perplexity), payload["perplexity"].as_f64());
            for member in ["entropies", "surprisals"] {
                let served: Vec<f64> = serde_json::from_str(pairs[member]).unwrap();
                let recorded: Vec<f64> = serde_json::from_value(payload[member].clone()).unwrap();
                assert_eq!(served, recorded, "{member}");
            }
        }
    }
}

/// **A message whose text carries U+0000 lands verbatim and serves back byte
/// for byte**, per `weaver-state-Spec` section 3's rule that a member lands
/// typed only where every engine can hold it. The service engine's `TEXT`
/// refuses the decoded NUL, so without the rule that engine rolls the
/// distillate back, and the embedded one would hold it typed where the other
/// could not. Each engine's caller then reads its own `field` table for the
/// verbatim row.
#[cfg(test)]
fn a_nul_in_a_message_lands_verbatim_and_serves_whole(store: &mut dyn crate::store::Store) {
    use crate::store::Distillate;
    let content = r#"[{"type":"text","text":"before\u0000after"}]"#;
    let sent = Distillate {
        session: "nul-session".into(),
        run: "nul-run".into(),
        turn: Some("t-1".into()),
        kind: "message.user".into(),
        sequence: 1,
        pairs: vec![
            ("content".into(), content.into()),
            ("role".into(), r#""user""#.into()),
        ],
    };
    store.land(&sent).expect("a NUL in a message lands");
    let served = store.replay("nul-session").expect("replays");
    assert_eq!(served.len(), 1);
    assert_eq!(served[0].pairs, sent.pairs, "served byte for byte");
}
