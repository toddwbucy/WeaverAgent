use super::*;
use std::os::fd::{AsRawFd, OwnedFd};

use nix::sys::socket::{AddressFamily, MsgFlags, SockFlag, SockType, recv, send, socketpair};
use weaver_trace::{Recorder, RunRef, SessionRef};
use weaver_types::{Finish, Generation, SessionId};

/// **A turn runs, and loop 0 authors the whole bracket.** Loop 1 supplies
/// the delta and receives the outcome, and the record carries the turn's
/// user message, the three model events, the assistant's turn, and the
/// **A refused classify authors a refusal and no output, and the case
/// keeps its values.** The refusal reached the record as a name until
/// 2026-08-22: `Oversized { requested, bound }` arrived as the word
/// "oversized", so a reader learned a bound was exceeded and never which
/// or by how much.
///
/// **The second assertion is the one that would rot quietly.** An
/// implementation that authored both would satisfy every claim about the
/// refusal while leaving `classify.output` meaning two things again,
/// which is the overloading the class was written to end.
#[test]
fn a_refused_classify_authors_a_refusal_and_no_output() {
    let session = SessionId("s-1".into());
    let sink = tempfile();
    let mut recorder = crate::record::Record::from(
        Recorder::receive(sink, RunRef("r-1".into()), SessionRef(session.0.clone()))
            .expect("recorder"),
    );
    let author = Author::new(&session, &weaver_types::RunId("r-1".into()));

    // The producer's own path, over the shape the seam hands it.
    let record = weaver_types::RefusalRecord::Classify {
        refusal: weaver_types::LabelRefusal::Oversized {
            requested: 9001,
            bound: 4096,
        },
    };
    let rendered = serde_json::to_string(&record).expect("the record renders");
    let payload = weaver_trace::raw_payload(&rendered).expect("it splices");
    author
        .author(
            &mut recorder,
            Kind::Refusal,
            Subsystem::Harness,
            None,
            Some(Payload::Refusal(payload)),
        )
        .expect("the refusal is authored");

    let refusals: Vec<&weaver_trace::Record> = recorder
        .structure()
        .expect("the serving record")
        .iter()
        .filter(|r| r.kind == Kind::Refusal)
        .collect();
    assert_eq!(refusals.len(), 1, "the refusal reached the record");
    let event: serde_json::Value =
        serde_json::from_str(refusals[0].line.as_ref()).expect("the line parses");
    assert_eq!(event["payload"]["seam"], "classify");
    assert_eq!(
        event["payload"]["refusal"]["requested"], 9001,
        "the case keeps the values a name would have dropped"
    );
    assert_eq!(event["payload"]["refusal"]["bound"], 4096);
    assert!(
        event["payload"].get("asked").is_none(),
        "classify carries no ask, its content standing in classify.request"
    );

    assert_eq!(
        recorder
            .structure()
            .expect("the serving record")
            .iter()
            .filter(|r| r.kind == Kind::ClassifyOutput)
            .count(),
        0,
        "a refused classify authors no output at all"
    );
}

/// **Pressure is reported once per crossing and not per turn above the
/// mark.** A fault for every turn over the mark answers a full queue by
/// filling it, which is the one direction that cannot help, and a
/// pressure report is a report about a condition rather than about an
/// event.
///
/// This drives the flag directly rather than filling a real queue: what
/// is under test is the once-per-crossing rule, and a test that pushed
/// 768 events to reach it would be testing the writer's arithmetic
/// instead.
///
/// Perturbation: drop the `pressure_reported` guard and the second and
/// third readings each author, three faults for one condition.
#[test]
fn pressure_reports_once_per_crossing() {
    let session = SessionId("s-1".into());
    let sink = tempfile();
    let mut recorder = crate::record::Record::from(
        Recorder::receive(sink, RunRef("r-1".into()), SessionRef(session.0.clone()))
            .expect("recorder"),
    );
    let author = Author::new(&session, &weaver_types::RunId("r-1".into()));
    let mut reported = false;

    // Under the mark: nothing is authored and the flag stays down.
    let faults = |r: &crate::record::Record| {
        r.structure()
            .expect("the serving record")
            .iter()
            .filter(|record| record.kind == Kind::Fault)
            .count()
    };
    let account = |queued: usize| {
        format!(
            "{{\"organ\":\"harness\",\"queued\":{queued},\"mark\":{}}}",
            weaver_trace::HIGH_WATER_MARK
        )
    };
    // The rule itself, exercised over the flag the engine holds: an
    // authored report sets it, a second crossing while it stands
    // authors nothing, and falling under the mark clears it.
    let mut report = |over: bool, queued: usize, recorder: &mut crate::record::Record| {
        if !over {
            reported = false;
            return;
        }
        if reported {
            return;
        }
        reported = true;
        let _ = author.author_fault(
            recorder,
            Subsystem::Harness,
            None,
            &crate::authorship::harness_report(
                weaver_types::FaultCase::RecorderCommitPressure,
                &account(queued),
            ),
        );
    };

    report(false, 12, &mut recorder);
    assert_eq!(faults(&recorder), 0, "under the mark authors nothing");

    report(true, 800, &mut recorder);
    assert_eq!(faults(&recorder), 1, "the crossing authors once");

    report(true, 900, &mut recorder);
    report(true, 1000, &mut recorder);
    assert_eq!(
        faults(&recorder),
        1,
        "a condition that persists is one condition"
    );

    report(false, 40, &mut recorder);
    report(true, 810, &mut recorder);
    assert_eq!(
        faults(&recorder),
        2,
        "falling under the mark and crossing again is a second condition"
    );
}

/// **The flag clears between turns and not only during one.** A queue
/// that drained and crossed again entirely between two turns is the case
/// the end-of-turn reading alone cannot see: it finds the depth high,
/// finds the flag still standing from the earlier crossing, and reports
/// nothing, so the second condition goes unrecorded.
///
/// The two readings are exercised in the order the turn runs them,
/// clear-only first and reporting last, over the flag itself.
///
/// Perturbation: drop the clear-only reading and the second crossing
/// authors nothing, the count staying at one.
#[test]
fn a_drain_between_turns_lets_the_next_crossing_report() {
    let mut reported = false;
    let mut authored = 0usize;

    let clear_only = |over: bool, flag: &mut bool| {
        if !over {
            *flag = false;
        }
    };
    let report = |over: bool, flag: &mut bool, count: &mut usize| {
        if !over {
            *flag = false;
            return;
        }
        if *flag {
            return;
        }
        *flag = true;
        *count += 1;
    };

    // Turn one: the queue is empty at its start and crosses by its end.
    clear_only(false, &mut reported);
    report(true, &mut reported, &mut authored);
    assert_eq!(authored, 1, "the first crossing reports");

    // Between the turns the sink drains and the queue refills past the
    // mark, so every reading a turn takes finds the depth high.
    clear_only(false, &mut reported);
    report(true, &mut reported, &mut authored);
    assert_eq!(
        authored, 2,
        "the drain was seen before the turn and the second crossing reports"
    );

    // And a turn that begins and ends above the mark reports nothing,
    // the condition never having lifted.
    clear_only(true, &mut reported);
    report(true, &mut reported, &mut authored);
    assert_eq!(
        authored, 2,
        "a condition that never lifted is still one condition"
    );
}

/// **A refused turn closes once, and the close says a refusal ended
/// it.** Every open has a close and the close says which kind it was,
/// per `weaver-trace-PRD` section 3.1, so two closes are as wrong as
/// none: a reader walking brackets meets a second close attributed to a
/// turn already ended, and the later one overwrites the earlier one's
/// account.
///
/// Perturbation: close at the failing arm as well as here, which is what
/// the first draft of this act did, and the count below reads two with
/// the second naming a fault. Watched under exactly that.
#[test]
fn a_refused_turn_closes_once_and_names_the_refusal() {
    let (near, far) = socketpair(
        AddressFamily::Unix,
        SockType::SeqPacket,
        None,
        SockFlag::SOCK_CLOEXEC,
    )
    .expect("socketpair");
    let decode = crate::channel::decode_from_owned(near);

    let peer = std::thread::spawn(move || {
        let mut buf = vec![0u8; 65536];
        let n = recv(far.as_raw_fd(), &mut buf, MsgFlags::empty()).expect("recv append");
        let directive: weaver_types::TokenDirective =
            serde_json::from_slice(&buf[..n]).expect("the append parses");
        let weaver_types::TokenDirective::AppendAndGenerate { tool_schemas, .. } = directive else {
            panic!("the turn sends append-and-generate");
        };
        assert!(
            tool_schemas.is_empty(),
            "the retiring engine preserves its rendering"
        );
        let refusal = weaver_types::TokenRefusal::Overflow {
            resident: 4091,
            requested: 137,
            capacity: 4096,
        };
        let bytes = serde_json::to_vec(&refusal).expect("the refusal renders");
        send(far.as_raw_fd(), &bytes, MsgFlags::empty()).expect("send refusal");
    });

    let session = SessionId("s-1".into());
    let sink = tempfile();
    let mut recorder = crate::record::Record::from(
        Recorder::receive(sink, RunRef("r-1".into()), SessionRef(session.0.clone()))
            .expect("recorder"),
    );
    let author = Author::new(&session, &weaver_types::RunId("r-1".into()));
    author
        .author(
            &mut recorder,
            Kind::Load,
            Subsystem::Harness,
            None,
            Some(Payload::Elections(Box::new(weaver_trace::Elections {
                residual_readout: false,
                field: None,
                surprisal: false,
                tee: Some(weaver_trace::Election::default()),
                state_member: false,
                declaration: Default::default(),
                lineage: None,
                reset: None,
                stack: Default::default(),
                boundary: String::new(),
                cause: weaver_trace::Cause { uid: 0 },
                library_path: None,
                state_store: Default::default(),
                composer: weaver_trace::LoopIdentity::compiled("test"),
            }))),
        )
        .expect("load");
    let mut turn_ordinal = 0u64;
    let mut turn_in_flight: Option<weaver_types::TurnKey> = None;
    let (listener, _dir) = test_listener();
    let outcome = {
        let mut fullness = None;
        let mut pressure_reported = false;
        let load_facts = crate::engine::test_load_facts();
        let mut ports = Ports::grant(
            &decode,
            &author,
            &mut recorder,
            &mut turn_ordinal,
            &mut turn_in_flight,
            &load_facts,
            None,
            &listener,
            None,
            None,
            None,
            None,
            &mut fullness,
            &mut pressure_reported,
        );
        ports.turn(vec![Message {
            role: Role::User,
            content: vec![ContentBlock::Text {
                text: "say a word".into(),
            }],
        }])
    };
    peer.join().expect("the decode peer finishes");
    assert!(
        matches!(outcome, Err(TurnError::Refused { .. })),
        "the refusal still reaches the caller"
    );

    let closes: Vec<&weaver_trace::Record> = recorder
        .structure()
        .expect("the serving record")
        .iter()
        .filter(|r| r.kind == Kind::TurnClosed)
        .collect();
    assert_eq!(closes.len(), 1, "one bracket, one close");
    let close: serde_json::Value =
        serde_json::from_str(closes[0].line.as_ref()).expect("the close parses");
    assert_eq!(close["payload"]["close"], "stopped");
    assert_eq!(
        close["payload"]["reason"], "refused",
        "and the reason names what ended the turn"
    );

    let refusals = recorder
        .structure()
        .expect("the serving record")
        .iter()
        .filter(|r| r.kind == Kind::Refusal)
        .count();
    assert_eq!(refusals, 1, "the refusal itself reached the record too");
}

/// **A refused ask reaches the record, and the values that ride are the
/// ones nothing else holds.** Before 2026-08-22 a refusal between turns
/// reached the loop as an absence and the record as nothing, so a reader
/// could not tell an ask turned away from an ask never made.
///
/// The scripted peer refuses an elision whose span is deliberately
/// unroundable, so a member arrived at by default fails here.
///
/// Perturbation: drop the `DecodeReply::Refusal` arm back to the
/// wildcard and the port answers `None` with the record silent, which is
/// the state this act exists to end. Watched under exactly that.
#[test]
fn a_refused_ask_reaches_the_record() {
    let (near, far) = socketpair(
        AddressFamily::Unix,
        SockType::SeqPacket,
        None,
        SockFlag::SOCK_CLOEXEC,
    )
    .expect("socketpair");
    let decode = crate::channel::decode_from_owned(near);

    let peer = std::thread::spawn(move || {
        let mut buf = vec![0u8; 65536];
        let n = recv(far.as_raw_fd(), &mut buf, MsgFlags::empty()).expect("recv elide");
        let directive: weaver_types::TokenDirective =
            serde_json::from_slice(&buf[..n]).expect("the elide parses");
        assert_eq!(
            directive,
            weaver_types::TokenDirective::Elide { from: 41, to: 57 }
        );
        let refusal = weaver_types::TokenRefusal::UnremovableSpan {
            from: 41,
            to: 57,
            prefix: 12,
            resident: 1237,
        };
        // The refusal crosses as itself: the seam frames an answer and a
        // refusal alike and the reader discriminates by parse.
        let bytes = serde_json::to_vec(&refusal).expect("the refusal renders");
        send(far.as_raw_fd(), &bytes, MsgFlags::empty()).expect("send refusal");
    });

    let session = SessionId("s-1".into());
    let sink = tempfile();
    let mut recorder = crate::record::Record::from(
        Recorder::receive(sink, RunRef("r-1".into()), SessionRef(session.0.clone()))
            .expect("recorder"),
    );
    let author = Author::new(&session, &weaver_types::RunId("r-1".into()));
    author
        .author(
            &mut recorder,
            Kind::Load,
            Subsystem::Harness,
            None,
            Some(Payload::Elections(Box::new(weaver_trace::Elections {
                residual_readout: false,
                field: None,
                surprisal: false,
                tee: Some(weaver_trace::Election::default()),
                state_member: false,
                declaration: Default::default(),
                lineage: None,
                reset: None,
                stack: Default::default(),
                boundary: String::new(),
                cause: weaver_trace::Cause { uid: 0 },
                library_path: None,
                state_store: Default::default(),
                composer: weaver_trace::LoopIdentity::compiled("test"),
            }))),
        )
        .expect("load");
    let mut turn_ordinal = 0u64;
    let mut turn_in_flight: Option<weaver_types::TurnKey> = None;
    let (listener, _dir) = test_listener();
    let answered = {
        let mut fullness = None;
        let mut pressure_reported = false;
        let load_facts = crate::engine::test_load_facts();
        let mut ports = Ports::grant(
            &decode,
            &author,
            &mut recorder,
            &mut turn_ordinal,
            &mut turn_in_flight,
            &load_facts,
            None,
            &listener,
            None,
            None,
            None,
            None,
            &mut fullness,
            &mut pressure_reported,
        );
        ports.elide(41, 57)
    };
    peer.join().expect("the decode peer finishes");
    assert!(answered.is_none(), "a refused span answers None as it did");

    let refusals: Vec<&weaver_trace::Record> = recorder
        .structure()
        .expect("the serving record")
        .iter()
        .filter(|r| r.kind == Kind::Refusal)
        .collect();
    assert_eq!(refusals.len(), 1, "the refusal reached the record");
    let event: serde_json::Value =
        serde_json::from_str(refusals[0].line.as_ref()).expect("the line parses");
    assert_eq!(event["payload"]["seam"], "decode");
    assert_eq!(
        event["payload"]["asked"]["ask"], "elide",
        "the ask is named"
    );
    assert_eq!(
        event["payload"]["asked"]["from"], 41,
        "and its span rides, no other event holding a span that was refused"
    );
    assert_eq!(event["payload"]["asked"]["to"], 57);
    assert_eq!(
        event["payload"]["refusal"]["prefix"], 12,
        "the seam's own case keeps its values"
    );
    assert!(
        event.get("turn").is_none(),
        "an elision is asked between turns, so its refusal belongs to none"
    );
}

/// **The elision's event is authored from the ask and not the answer.**
/// The seam echoes no span, per the decode contract, so the only place
/// the record can get one is what the loop named. A site reading the
/// span off the answer would have nothing to read, and a site
/// defaulting it would write a record of a removal that did not happen
/// where one did.
///
/// The scripted peer answers counts that are deliberately unroundable
/// and no span at all, the `Elided` variant having none to carry, so a
/// member arrived at by accident or by default fails here.
///
/// conforms: harness-elision-authors-from-the-ask
#[test]
fn the_elision_event_carries_the_span_the_loop_named() {
    let (near, far) = socketpair(
        AddressFamily::Unix,
        SockType::SeqPacket,
        None,
        SockFlag::SOCK_CLOEXEC,
    )
    .expect("socketpair");
    let decode = crate::channel::decode_from_owned(near);

    let peer = std::thread::spawn(move || {
        let mut buf = vec![0u8; 65536];
        let n = recv(far.as_raw_fd(), &mut buf, MsgFlags::empty()).expect("recv elide");
        let directive: weaver_types::TokenDirective =
            serde_json::from_slice(&buf[..n]).expect("the elide parses");
        assert_eq!(
            directive,
            weaver_types::TokenDirective::Elide { from: 41, to: 57 },
            "the port forwards the span unjudged"
        );
        let answer = weaver_types::TokenAnswer::Elided {
            resident_before: 1237,
            resident_after: 1221,
        };
        let bytes = serde_json::to_vec(&answer).expect("answer renders");
        send(far.as_raw_fd(), &bytes, MsgFlags::empty()).expect("send answer");
    });

    let session = SessionId("s-1".into());
    let sink = tempfile();
    let mut recorder = crate::record::Record::from(
        Recorder::receive(sink, RunRef("r-1".into()), SessionRef(session.0.clone()))
            .expect("recorder"),
    );
    let author = Author::new(&session, &weaver_types::RunId("r-1".into()));
    author
        .author(
            &mut recorder,
            Kind::Load,
            Subsystem::Harness,
            None,
            Some(Payload::Elections(Box::new(weaver_trace::Elections {
                residual_readout: false,
                field: None,
                surprisal: false,
                tee: Some(weaver_trace::Election::default()),
                state_member: false,
                declaration: Default::default(),
                lineage: None,
                reset: None,
                stack: Default::default(),
                boundary: String::new(),
                cause: weaver_trace::Cause { uid: 0 },
                library_path: None,
                state_store: Default::default(),
                composer: weaver_trace::LoopIdentity::compiled("test"),
            }))),
        )
        .expect("load");
    let mut turn_ordinal = 0u64;
    let mut turn_in_flight: Option<weaver_types::TurnKey> = None;
    let (listener, _dir) = test_listener();
    let counts = {
        let mut fullness = Some((1237, 8191));
        let mut pressure_reported = false;
        let load_facts = crate::engine::test_load_facts();
        let mut ports = Ports::grant(
            &decode,
            &author,
            &mut recorder,
            &mut turn_ordinal,
            &mut turn_in_flight,
            &load_facts,
            None,
            &listener,
            None,
            None,
            None,
            None,
            &mut fullness,
            &mut pressure_reported,
        );
        let counts = ports.elide(41, 57).expect("the elision holds");
        assert_eq!(
            fullness,
            Some((1221, 8191)),
            "the fullness follows the shortened state"
        );
        counts
    };
    peer.join().expect("the decode peer finishes");
    assert_eq!(counts, (1237, 1221), "the seam's counts reach the loop");

    let lines: Vec<&weaver_trace::Record> = recorder
        .structure()
        .expect("the serving record")
        .iter()
        .filter(|r| r.kind == Kind::Elision)
        .collect();
    assert_eq!(lines.len(), 1, "one elision, one event");
    let event: serde_json::Value =
        serde_json::from_str(lines[0].line.as_ref()).expect("the line parses");
    assert_eq!(event["payload"]["from"], 41, "the span comes from the ask");
    assert_eq!(event["payload"]["to"], 57, "both bounds of it");
    assert_eq!(
        event["payload"]["resident_before"], 1237,
        "and the counts come from the answer"
    );
    assert_eq!(event["payload"]["resident_after"], 1221);
    assert!(
        event.get("turn").is_none(),
        "an elision is asked between turns and belongs to none"
    );
}

/// close, in that order. The decode peer is scripted here rather than a
/// real SPU: it streams two tokens and then the generation whole, which is
/// what the turn method consumes and authors.
#[test]
fn a_turn_authors_the_whole_bracket() {
    let (near, far) = socketpair(
        AddressFamily::Unix,
        SockType::SeqPacket,
        None,
        SockFlag::SOCK_CLOEXEC,
    )
    .expect("socketpair");
    let decode = crate::channel::decode_from_owned(near);

    // The scripted decode peer: read the append, stream two tokens, then
    // answer with the generation whole, the request and measurement the
    // SPU-rendered blobs the turn splices.
    let peer = std::thread::spawn(move || {
        let mut buf = vec![0u8; 65536];
        let n = recv(far.as_raw_fd(), &mut buf, MsgFlags::empty()).expect("recv append");
        let directive: weaver_types::TokenDirective =
            serde_json::from_slice(&buf[..n]).expect("the append parses");
        assert!(
            matches!(
                directive,
                weaver_types::TokenDirective::AppendAndGenerate { .. }
            ),
            "the turn sends append-and-generate"
        );
        let send_answer = |answer: &weaver_types::TokenAnswer| {
            let bytes = serde_json::to_vec(answer).expect("answer renders");
            send(far.as_raw_fd(), &bytes, MsgFlags::empty()).expect("send answer");
        };
        send_answer(&weaver_types::TokenAnswer::Token {
            token: 9707,
            piece: "one".into(),
        });
        send_answer(&weaver_types::TokenAnswer::Token {
            token: 1879,
            piece: " word".into(),
        });
        let request = serde_json::value::RawValue::from_string(
            r#"{"rendered":"<|im_start|>user\nsay a word<|im_end|>\n","template":"qwen2","sampling":{"temperature":0.7}}"#
                .to_string(),
        )
        .unwrap();
        let measurement = serde_json::value::RawValue::from_string(
            r#"{"model":"qwen2.5","weights_hash":"sha256:abc","input_tokens":[1,2],"output_tokens":[9707,1879],"blocks":[{"label":"turn-delta","start":0,"end":10}],"timings":{"prefill_ns":"10","decode_ns":"20"}}"#
                .to_string(),
        )
        .unwrap();
        send_answer(&weaver_types::TokenAnswer::Generated(Generation {
            unrecovered: vec![],
            content: vec![weaver_traits::ContentBlock::Text {
                text: "one word".into(),
            }],
            emission: "one word".into(),
            finish: Finish::Completed,
            // Deliberately unroundable: a site forwarding a zero, a
            // default, or the other member would fail the assertion
            // below, where 64 and 4096 could each be arrived at by
            // accident.
            resident: 1237,
            capacity: 8191,
            request,
            measurement,
        }));
    });

    let session = SessionId("s-1".into());
    let sink = tempfile();
    let mut recorder = crate::record::Record::from(
        Recorder::receive(sink, RunRef("r-1".into()), SessionRef(session.0.clone()))
            .expect("recorder"),
    );
    let author = Author::new(&session, &weaver_types::RunId("r-1".into()));
    author
        .author(
            &mut recorder,
            Kind::Load,
            Subsystem::Harness,
            None,
            Some(Payload::Elections(Box::new(weaver_trace::Elections {
                residual_readout: false,
                field: None,
                surprisal: false,
                tee: Some(weaver_trace::Election::default()),
                state_member: false,
                declaration: Default::default(),
                lineage: None,
                reset: None,
                stack: Default::default(),
                boundary: String::new(),
                cause: weaver_trace::Cause { uid: 0 },
                library_path: None,
                state_store: Default::default(),
                composer: weaver_trace::LoopIdentity::compiled("test"),
            }))),
        )
        .expect("load");
    let mut turn_ordinal = 0u64;
    let mut turn_in_flight: Option<weaver_types::TurnKey> = None;

    let (listener, _dir) = test_listener();
    let outcome = {
        let mut fullness = None;
        let mut pressure_reported = false;
        let load_facts = crate::engine::test_load_facts();
        let mut ports = Ports::grant(
            &decode,
            &author,
            &mut recorder,
            &mut turn_ordinal,
            &mut turn_in_flight,
            &load_facts,
            None,
            &listener,
            None,
            None,
            None,
            None,
            &mut fullness,
            &mut pressure_reported,
        );
        let delta = vec![Message {
            role: Role::User,
            content: vec![ContentBlock::Text {
                text: "say a word".into(),
            }],
        }];
        ports.turn(delta).expect("the turn completes")
    };
    peer.join().expect("the decode peer finishes");

    assert_eq!(outcome.emission, "one word", "loop 1 receives the emission");
    assert!(!outcome.stopped, "a clean generation is not stopped");

    // The bracket, in order: the turn's user message, the three model
    // events, the assistant's turn, and the close.
    let kinds: Vec<Kind> = recorder
        .structure()
        .expect("the serving record")
        .iter()
        .filter(|r| r.turn.is_some())
        .map(|r| r.kind)
        .collect();
    assert_eq!(
        kinds,
        vec![
            Kind::TurnStarted,
            Kind::MessageUser,
            Kind::ModelRequest,
            Kind::ModelOutput,
            Kind::ModelMeasurement,
            Kind::MessageAssistant,
            Kind::TurnClosed,
        ],
        "the whole bracket authored in order"
    );

    // **The output's counts are the generation's**, per
    // `weaver-trace-Spec` section 3. The trace crate's own test proves
    // the pair serializes; this one proves the authoring site forwards
    // what the seam delivered rather than a default, which is the half
    // that lives in this crate.
    //
    // Perturbation: forward a zero, a constant, or the members swapped,
    // and this fails.
    let output_line = recorder
        .structure()
        .expect("the serving record")
        .by_kind(Kind::ModelOutput)
        .next()
        .expect("the output authored")
        .line
        .to_string();
    let output: serde_json::Value =
        serde_json::from_str(&output_line).expect("the output line is one value");
    assert_eq!(
        output["payload"]["resident"], 1237,
        "the generation's resident count reaches the record: {output_line}"
    );
    assert_eq!(
        output["payload"]["capacity"], 8191,
        "and its capacity: {output_line}"
    );

    // The model events splice what the peer rendered: the request carries
    // the template, the measurement the weights hash, both opaque.
    let line_of = |kind: Kind| {
        recorder
            .structure()
            .expect("the serving record")
            .by_kind(kind)
            .next()
            .expect("the event authored")
            .line
            .to_string()
    };
    assert!(
        line_of(Kind::ModelRequest).contains("\"template\":\"qwen2\""),
        "the request splice carries what the SPU rendered"
    );
    assert!(
        line_of(Kind::ModelMeasurement).contains("sha256:abc"),
        "the measurement splice carries what the SPU rendered"
    );

    // The attribution, at engine grain: all three model events carry the
    // decoder's case on the wire, per the #103 ruling. Read from the
    // rendered line rather than the enum so a regression to `Spu` at the
    // author call fails here naming the string, which is the emitter-side
    // half of the pin whose spelling half lives in the recorder's tests.
    for kind in [
        Kind::ModelRequest,
        Kind::ModelOutput,
        Kind::ModelMeasurement,
    ] {
        assert!(
            line_of(kind).contains("\"subsystem\":\"spu_decoder\""),
            "{kind:?} is attributed to the decode engine, got {}",
            line_of(kind)
        );
    }
}

/// The emission mid-stream: the report is authored inside the bracket,
/// before the close. The pin is the order: a turn-attributed `fault`
/// filed after `turn.closed` would sit outside the bracket that names
/// it, which is the defect the wrapper's author-then-close sequence
/// exists to prevent.
#[test]
fn a_fault_emission_lands_inside_the_bracket_before_the_close() {
    let (near, far) = socketpair(
        AddressFamily::Unix,
        SockType::SeqPacket,
        None,
        SockFlag::SOCK_CLOEXEC,
    )
    .expect("socketpair");
    let decode = crate::channel::decode_from_owned(near);

    // The scripted peer: read the append, stream one token, then emit
    // the fault instead of the generation.
    let peer = std::thread::spawn(move || {
        let mut buf = vec![0u8; 65536];
        let _ = recv(far.as_raw_fd(), &mut buf, MsgFlags::empty()).expect("recv append");
        let send_answer = |answer: &weaver_types::TokenAnswer| {
            let bytes = serde_json::to_vec(answer).expect("answer renders");
            send(far.as_raw_fd(), &bytes, MsgFlags::empty()).expect("send answer");
        };
        send_answer(&weaver_types::TokenAnswer::Token {
            token: 9707,
            piece: "one".into(),
        });
        send_answer(&weaver_types::TokenAnswer::Fault(
            weaver_types::FaultReport {
                case: weaver_types::FaultCase::DeviceFaultDuringGeneration,
                account: serde_json::value::RawValue::from_string(
                    r#"{"device":"cuda:0","errored":"mid-forward"}"#.to_string(),
                )
                .unwrap(),
            },
        ));
    });

    let session = SessionId("s-1".into());
    let sink = tempfile();
    let mut recorder = crate::record::Record::from(
        Recorder::receive(sink, RunRef("r-1".into()), SessionRef(session.0.clone()))
            .expect("recorder"),
    );
    let author = Author::new(&session, &weaver_types::RunId("r-1".into()));
    author
        .author(
            &mut recorder,
            Kind::Load,
            Subsystem::Harness,
            None,
            Some(Payload::Elections(Box::new(weaver_trace::Elections {
                residual_readout: false,
                field: None,
                surprisal: false,
                tee: Some(weaver_trace::Election::default()),
                state_member: false,
                declaration: Default::default(),
                lineage: None,
                reset: None,
                stack: Default::default(),
                boundary: String::new(),
                cause: weaver_trace::Cause { uid: 0 },
                library_path: None,
                state_store: Default::default(),
                composer: weaver_trace::LoopIdentity::compiled("test"),
            }))),
        )
        .expect("load");
    let mut turn_ordinal = 0u64;
    let mut turn_in_flight: Option<weaver_types::TurnKey> = None;

    let (listener, _dir) = test_listener();
    let error = {
        let mut fullness = None;
        let mut pressure_reported = false;
        let load_facts = crate::engine::test_load_facts();
        let mut ports = Ports::grant(
            &decode,
            &author,
            &mut recorder,
            &mut turn_ordinal,
            &mut turn_in_flight,
            &load_facts,
            None,
            &listener,
            None,
            None,
            None,
            None,
            &mut fullness,
            &mut pressure_reported,
        );
        let delta = vec![Message {
            role: Role::User,
            content: vec![ContentBlock::Text {
                text: "say a word".into(),
            }],
        }];
        ports.turn(delta).expect_err("the emission fails the turn")
    };
    peer.join().expect("the decode peer finishes");

    let TurnError::Faulted { turn, report } = error else {
        panic!("the emission is matched by name, got another error");
    };
    assert_eq!(turn.0, "t-1", "the streaming turn is the one named");
    assert_eq!(
        report.case,
        weaver_types::FaultCase::DeviceFaultDuringGeneration,
        "the report rides the error for the caller's close"
    );

    // The pin: the turn-attributed sequence holds the fault before the
    // close, both inside the bracket.
    let kinds: Vec<Kind> = recorder
        .structure()
        .expect("the serving record")
        .iter()
        .filter(|r| r.turn.is_some())
        .map(|r| r.kind)
        .collect();
    let fault_at = kinds
        .iter()
        .position(|k| *k == Kind::Fault)
        .expect("the report was authored");
    let close_at = kinds
        .iter()
        .position(|k| *k == Kind::TurnClosed)
        .expect("the bracket closed");
    assert!(
        fault_at < close_at,
        "the fault lands inside the bracket, before the close: {kinds:?}"
    );
}

/// **A turn executes its calls, and the grant authors the result.** The
/// whole mechanism in one bracket: the scripted decode peer's first
/// generation carries a `ToolCall` block, the scripted gate peer answers
/// the execution exchange with the result, the grant authors the
/// tool-result turn at its one door, the same content crosses back as
/// the second delta, and the second generation closes the turn as text.
///
/// What the bracket must read, in order: the user message, the first
/// model triplet, the assistant turn carrying the call, the tool
/// bracket's two events, the tool-result message, the second model
/// triplet, the closing assistant turn, and the close - two generations
/// inside one turn, which is the loop the tool workflow exists for.
///
/// Perturbation: drop the `execute_call` loop and route calls as plain
/// text, and this fails at the kinds assertion missing the tool bracket.
#[test]
fn a_turn_executes_its_calls_and_the_grant_authors_the_result() {
    let (near, far) = socketpair(
        AddressFamily::Unix,
        SockType::SeqPacket,
        None,
        SockFlag::SOCK_CLOEXEC,
    )
    .expect("socketpair");
    let decode = crate::channel::decode_from_owned(near);

    // The decode peer: two rounds. Round one answers a generation whose
    // canonical content carries the call; round two reads the tool-result
    // delta back and answers plain text.
    let peer = std::thread::spawn(move || {
        let mut buf = vec![0u8; 65536];
        let raw = |text: &str| serde_json::value::RawValue::from_string(text.to_string()).unwrap();
        let send_answer = |answer: &weaver_types::TokenAnswer| {
            let bytes = serde_json::to_vec(answer).expect("answer renders");
            send(far.as_raw_fd(), &bytes, MsgFlags::empty()).expect("send answer");
        };

        let n = recv(far.as_raw_fd(), &mut buf, MsgFlags::empty()).expect("recv append");
        let directive: weaver_types::TokenDirective =
            serde_json::from_slice(&buf[..n]).expect("the append parses");
        assert!(matches!(
            directive,
            weaver_types::TokenDirective::AppendAndGenerate { .. }
        ));
        send_answer(&weaver_types::TokenAnswer::Generated(Generation {
            unrecovered: vec![],
            content: vec![weaver_traits::ContentBlock::ToolCall(
                weaver_traits::ToolCall {
                    name: "sh".into(),
                    arguments: r#"{"expression":"37 * 43"}"#.into(),
                },
            )],
            emission: r#"<tool_call>{"name":"sh"}</tool_call>"#.into(),
            finish: Finish::Completed,
            resident: 64,
            capacity: 4096,
            request: raw(r#"{"rendered":"r1","template":"qwen2","sampling":{}}"#),
            measurement: raw(
                r#"{"model":"m","weights_hash":"h","input_tokens":[1],"output_tokens":[2],"blocks":[{"label":"turn-delta","start":0,"end":1}],"timings":{"prefill_ns":"1","decode_ns":"2"}}"#,
            ),
        }));

        // Round two: the tool-result delta arrives, carrying the granted
        // content and nothing the loop invented.
        let n = recv(far.as_raw_fd(), &mut buf, MsgFlags::empty()).expect("recv round 2");
        let directive: weaver_types::TokenDirective =
            serde_json::from_slice(&buf[..n]).expect("round 2 parses");
        let weaver_types::TokenDirective::AppendAndGenerate { delta, .. } = directive else {
            panic!("round two is an append");
        };
        assert_eq!(delta.len(), 1, "one tool-result message crosses");
        assert!(matches!(delta[0].role, Role::ToolResult));
        assert!(
            matches!(
                &delta[0].content[0],
                ContentBlock::ToolResult(block) if block.content == "1591"
            ),
            "the granted content is what crosses: {:?}",
            delta[0].content
        );
        send_answer(&weaver_types::TokenAnswer::Generated(Generation {
            unrecovered: vec![],
            content: vec![weaver_traits::ContentBlock::Text {
                text: "37 * 43 is 1591.".into(),
            }],
            emission: "37 * 43 is 1591.".into(),
            finish: Finish::Completed,
            resident: 64,
            capacity: 4096,
            request: raw(r#"{"rendered":"r2","template":"qwen2","sampling":{}}"#),
            measurement: raw(
                r#"{"model":"m","weights_hash":"h","input_tokens":[3],"output_tokens":[4],"blocks":[{"label":"turn-delta","start":0,"end":1}],"timings":{"prefill_ns":"1","decode_ns":"2"}}"#,
            ),
        }));
    });

    // The gate peer: one execution exchange, answered with the result,
    // exactly as the gate's dispatch would.
    let (gate_near, gate_child) = crate::channel::OrganChannel::pair().expect("gate pair");
    let gate_peer = std::thread::spawn(move || {
        let gate_far = gate_child.into_channel();
        let envelope = gate_far.recv().expect("the execution opens");
        assert_eq!(envelope.position, weaver_types::Position::Open);
        let weaver_types::Payload::Tool(execution) = &envelope.payload else {
            panic!("the exchange carries the call, got {:?}", envelope.payload);
        };
        assert_eq!(execution.name.0, "sh");
        let outcome = weaver_gate_execute_stub(execution);
        gate_far
            .send(&weaver_types::OrganEnvelope {
                exchange: envelope.exchange.clone(),
                position: weaver_types::Position::Close,
                payload: weaver_types::Payload::ToolAnswer(outcome),
            })
            .expect("the answer closes");
    });

    let session = SessionId("s-t".into());
    let sink = tempfile();
    let mut recorder = crate::record::Record::from(
        Recorder::receive(sink, RunRef("r-1".into()), SessionRef(session.0.clone()))
            .expect("recorder"),
    );
    let author = Author::new(&session, &weaver_types::RunId("r-1".into()));
    author
        .author(
            &mut recorder,
            Kind::Load,
            Subsystem::Harness,
            None,
            Some(Payload::Elections(Box::new(weaver_trace::Elections {
                residual_readout: false,
                field: None,
                surprisal: false,
                tee: Some(weaver_trace::Election::default()),
                state_member: false,
                declaration: Default::default(),
                lineage: None,
                reset: None,
                stack: Default::default(),
                boundary: String::new(),
                cause: weaver_trace::Cause { uid: 0 },
                library_path: None,
                state_store: Default::default(),
                composer: weaver_trace::LoopIdentity::compiled("test"),
            }))),
        )
        .expect("load");
    let mut turn_ordinal = 0u64;
    let mut turn_in_flight: Option<weaver_types::TurnKey> = None;
    let mut gate_ordinal = 0u64;
    let mut held = std::collections::VecDeque::new();

    let (listener, _dir) = test_listener();
    let outcome = {
        let mut fullness = None;
        let mut pressure_reported = false;
        let load_facts = crate::engine::test_load_facts();
        let mut ports = Ports::grant(
            &decode,
            &author,
            &mut recorder,
            &mut turn_ordinal,
            &mut turn_in_flight,
            &load_facts,
            None,
            &listener,
            None,
            Some(GatePort {
                channel: &gate_near,
                ordinal: &mut gate_ordinal,
                held: &mut held,
                tools: None,
                contained: None,
                shell_granted: true,
            }),
            None,
            None,
            &mut fullness,
            &mut pressure_reported,
        );
        let delta = vec![Message {
            role: Role::User,
            content: vec![ContentBlock::Text {
                text: "what is 37 * 43?".into(),
            }],
        }];
        ports.turn(delta).expect("the turn completes")
    };
    peer.join().expect("the decode peer finishes");
    gate_peer.join().expect("the gate peer finishes");

    assert_eq!(outcome.emission, "37 * 43 is 1591.");
    assert!(held.is_empty(), "nothing crossed mid-execution to hold");

    let kinds: Vec<Kind> = recorder
        .structure()
        .expect("the serving record")
        .iter()
        .filter(|r| r.turn.is_some())
        .map(|r| r.kind)
        .collect();
    assert_eq!(
        kinds,
        vec![
            Kind::TurnStarted,
            Kind::MessageUser,
            Kind::ModelRequest,
            Kind::ModelOutput,
            Kind::ModelMeasurement,
            Kind::MessageAssistant,
            Kind::ToolCallStarted,
            Kind::ToolCallCompleted,
            Kind::MessageToolResult,
            Kind::ModelRequest,
            Kind::ModelOutput,
            Kind::ModelMeasurement,
            Kind::MessageAssistant,
            Kind::TurnClosed,
        ],
        "two generations inside one bracket, the tool loop between them"
    );

    // The attribution: started is the harness's dispatch, completed is
    // the gate's answer.
    let line_of = |kind: Kind| {
        recorder
            .structure()
            .expect("the serving record")
            .by_kind(kind)
            .next()
            .expect("authored")
            .line
            .to_string()
    };
    assert!(line_of(Kind::ToolCallStarted).contains("\"subsystem\":\"harness\""));
    assert!(line_of(Kind::ToolCallCompleted).contains("\"subsystem\":\"gate\""));
    assert!(
        line_of(Kind::MessageToolResult).contains("1591"),
        "the granted content is the record's"
    );
}

/// **A call the enter did not grant closes `Refused` in this crate, and
/// nothing crosses the gate** (#29, `weaver-harness-Spec` section 6): the
/// tool bracket's two events are both the harness's, the refusal names the
/// call as not elected, and the tool-result turn carries it to the model.
/// Two cases: the shell's own name with no grant, and a name that is not the
/// shell's where the shell is granted.
///
/// Perturbation: drop the grant check from `execute_call` and the call opens
/// an exchange, failing the gate peer.
#[test]
fn an_ungranted_shell_call_closes_refused_in_the_harness() {
    an_ungranted_call("sh", false);
}

/// The second case of the test above.
#[test]
fn an_unelected_name_closes_refused_in_the_harness() {
    an_ungranted_call("bash", true);
}

fn an_ungranted_call(name: &str, shell_granted: bool) {
    let name = name.to_string();
    let refused_name = name.clone();
    let (near, far) = socketpair(
        AddressFamily::Unix,
        SockType::SeqPacket,
        None,
        SockFlag::SOCK_CLOEXEC,
    )
    .expect("socketpair");
    let decode = crate::channel::decode_from_owned(near);

    // The decode peer: two rounds. Round one answers a generation whose
    // canonical content carries the call, and round two reads the tool-result
    // delta back and answers plain text.
    let peer = std::thread::spawn(move || {
        let mut buf = vec![0u8; 65536];
        let raw = |text: &str| serde_json::value::RawValue::from_string(text.to_string()).unwrap();
        let send_answer = |answer: &weaver_types::TokenAnswer| {
            let bytes = serde_json::to_vec(answer).expect("answer renders");
            send(far.as_raw_fd(), &bytes, MsgFlags::empty()).expect("send answer");
        };

        let n = recv(far.as_raw_fd(), &mut buf, MsgFlags::empty()).expect("recv append");
        let directive: weaver_types::TokenDirective =
            serde_json::from_slice(&buf[..n]).expect("the append parses");
        assert!(matches!(
            directive,
            weaver_types::TokenDirective::AppendAndGenerate { .. }
        ));
        send_answer(&weaver_types::TokenAnswer::Generated(Generation {
            unrecovered: vec![],
            content: vec![weaver_traits::ContentBlock::ToolCall(
                weaver_traits::ToolCall {
                    name: name.clone(),
                    arguments: r#"{"command":"ls"}"#.into(),
                },
            )],
            emission: r#"<tool_call>{"name":"sh"}</tool_call>"#.into(),
            finish: Finish::Completed,
            resident: 64,
            capacity: 4096,
            request: raw(r#"{"rendered":"r1","template":"qwen2","sampling":{}}"#),
            measurement: raw(
                r#"{"model":"m","weights_hash":"h","input_tokens":[1],"output_tokens":[2],"blocks":[{"label":"turn-delta","start":0,"end":1}],"timings":{"prefill_ns":"1","decode_ns":"2"}}"#,
            ),
        }));

        // Round two: the tool-result delta arrives, carrying the granted
        // content and nothing the loop invented.
        let n = recv(far.as_raw_fd(), &mut buf, MsgFlags::empty()).expect("recv round 2");
        let directive: weaver_types::TokenDirective =
            serde_json::from_slice(&buf[..n]).expect("round 2 parses");
        let weaver_types::TokenDirective::AppendAndGenerate { delta, .. } = directive else {
            panic!("round two is an append");
        };
        assert_eq!(delta.len(), 1, "one tool-result message crosses");
        assert!(matches!(delta[0].role, Role::ToolResult));
        assert!(
            matches!(
                &delta[0].content[0],
                ContentBlock::ToolResult(block)
                    if block.content.contains("is not a tool this agent's declaration elects")
                        && block.content.contains(&name)
            ),
            "the harness's refusal is what crosses, naming the call: {:?}",
            delta[0].content
        );
        send_answer(&weaver_types::TokenAnswer::Generated(Generation {
            unrecovered: vec![],
            content: vec![weaver_traits::ContentBlock::Text {
                text: "refused.".into(),
            }],
            emission: "refused.".into(),
            finish: Finish::Completed,
            resident: 64,
            capacity: 4096,
            request: raw(r#"{"rendered":"r2","template":"qwen2","sampling":{}}"#),
            measurement: raw(
                r#"{"model":"m","weights_hash":"h","input_tokens":[3],"output_tokens":[4],"blocks":[{"label":"turn-delta","start":0,"end":1}],"timings":{"prefill_ns":"1","decode_ns":"2"}}"#,
            ),
        }));
    });

    // The gate peer: **nothing crosses**. The channel closes when the
    // harness's end drops after the turn, and no envelope precedes it.
    let (gate_near, gate_child) = crate::channel::OrganChannel::pair().expect("gate pair");
    let gate_peer = std::thread::spawn(move || {
        let gate_far = gate_child.into_channel();
        let got = gate_far.recv();
        assert!(got.is_err(), "no execution opens at the gate: {got:?}");
    });

    let session = SessionId("s-t".into());
    let sink = tempfile();
    let mut recorder = crate::record::Record::from(
        Recorder::receive(sink, RunRef("r-1".into()), SessionRef(session.0.clone()))
            .expect("recorder"),
    );
    let author = Author::new(&session, &weaver_types::RunId("r-1".into()));
    author
        .author(
            &mut recorder,
            Kind::Load,
            Subsystem::Harness,
            None,
            Some(Payload::Elections(Box::new(weaver_trace::Elections {
                residual_readout: false,
                field: None,
                surprisal: false,
                tee: Some(weaver_trace::Election::default()),
                state_member: false,
                declaration: Default::default(),
                lineage: None,
                reset: None,
                stack: Default::default(),
                boundary: String::new(),
                cause: weaver_trace::Cause { uid: 0 },
                library_path: None,
                state_store: Default::default(),
                composer: weaver_trace::LoopIdentity::compiled("test"),
            }))),
        )
        .expect("load");
    let mut turn_ordinal = 0u64;
    let mut turn_in_flight: Option<weaver_types::TurnKey> = None;
    let mut gate_ordinal = 0u64;
    let mut held = std::collections::VecDeque::new();

    let (listener, _dir) = test_listener();
    let outcome = {
        let mut fullness = None;
        let mut pressure_reported = false;
        let load_facts = crate::engine::test_load_facts();
        let mut ports = Ports::grant(
            &decode,
            &author,
            &mut recorder,
            &mut turn_ordinal,
            &mut turn_in_flight,
            &load_facts,
            None,
            &listener,
            None,
            Some(GatePort {
                channel: &gate_near,
                ordinal: &mut gate_ordinal,
                held: &mut held,
                tools: None,
                contained: None,
                shell_granted,
            }),
            None,
            None,
            &mut fullness,
            &mut pressure_reported,
        );
        let delta = vec![Message {
            role: Role::User,
            content: vec![ContentBlock::Text {
                text: "what is 37 * 43?".into(),
            }],
        }];
        ports.turn(delta).expect("the turn completes")
    };
    peer.join().expect("the decode peer finishes");
    drop(gate_near);
    gate_peer.join().expect("the gate peer finishes");

    assert_eq!(outcome.emission, "refused.");
    assert!(held.is_empty(), "nothing crossed mid-execution to hold");

    let kinds: Vec<Kind> = recorder
        .structure()
        .expect("the serving record")
        .iter()
        .filter(|r| r.turn.is_some())
        .map(|r| r.kind)
        .collect();
    assert_eq!(
        kinds,
        vec![
            Kind::TurnStarted,
            Kind::MessageUser,
            Kind::ModelRequest,
            Kind::ModelOutput,
            Kind::ModelMeasurement,
            Kind::MessageAssistant,
            Kind::ToolCallStarted,
            Kind::ToolCallCompleted,
            Kind::MessageToolResult,
            Kind::ModelRequest,
            Kind::ModelOutput,
            Kind::ModelMeasurement,
            Kind::MessageAssistant,
            Kind::TurnClosed,
        ],
        "two generations inside one bracket, the tool loop between them"
    );

    // The attribution: started and completed are both the harness's, the
    // refusal its own word.
    let line_of = |kind: Kind| {
        recorder
            .structure()
            .expect("the serving record")
            .by_kind(kind)
            .next()
            .expect("authored")
            .line
            .to_string()
    };
    assert!(line_of(Kind::ToolCallStarted).contains("\"subsystem\":\"harness\""));
    let completed = line_of(Kind::ToolCallCompleted);
    assert!(
        completed.contains("\"subsystem\":\"harness\""),
        "{completed}"
    );
    assert!(completed.contains("refused"), "{completed}");
    assert!(
        line_of(Kind::MessageToolResult).contains(&refused_name),
        "the refusal is the record's, naming the call"
    );
}

/// **Each field intermediate authors one `model.field` event**, per
/// `weaver-harness-Spec` section 6. The intermediates close nothing, so
/// the turn runs on and the close arrives as it always did: the record
/// gains the field's events and loses none of the bracket.
///
/// Perturbation: discard the field arm the way the token arm is
/// discarded and the kinds assertion fails, the events never authored.
#[test]
fn each_field_intermediate_authors_its_event() {
    let (near, far) = socketpair(
        AddressFamily::Unix,
        SockType::SeqPacket,
        None,
        SockFlag::SOCK_CLOEXEC,
    )
    .expect("socketpair");

    let decode = crate::channel::decode_from_owned(near);
    let peer = std::thread::spawn(move || {
        let mut buf = vec![0u8; 65536];
        let _ = recv(far.as_raw_fd(), &mut buf, MsgFlags::empty()).expect("recv append");
        let send_answer = |answer: &weaver_types::TokenAnswer| {
            let bytes = serde_json::to_vec(answer).expect("answer renders");
            send(far.as_raw_fd(), &bytes, MsgFlags::empty()).expect("send answer");
        };
        // Two positions of field, and a token intermediate between them
        // to prove the two streams interleave without either being lost.
        for position in [0u64, 1] {
            send_answer(&weaver_types::TokenAnswer::Field {
                position,
                ranked: vec![
                    weaver_types::Candidate {
                        token: 11,
                        probability: 0.75,
                    },
                    weaver_types::Candidate {
                        token: 12,
                        probability: 0.25,
                    },
                ],
                realized: 0,
            });
            send_answer(&weaver_types::TokenAnswer::Token {
                token: 11,
                piece: "hi".into(),
            });
        }
        let request = serde_json::value::RawValue::from_string(
            r#"{"rendered":"x","template":"t","sampling":{"seed":11}}"#.to_string(),
        )
        .unwrap();
        let measurement = serde_json::value::RawValue::from_string(
            r#"{"model":"m","weights_hash":"h","input_tokens":[1],"output_tokens":[11],"blocks":[{"label":"turn-delta","start":0,"end":1}],"timings":{"prefill_ns":"1","decode_ns":"2"}}"#
                .to_string(),
        )
        .unwrap();
        send_answer(&weaver_types::TokenAnswer::Generated(Generation {
            unrecovered: vec![],
            content: vec![weaver_traits::ContentBlock::Text { text: "hi".into() }],
            emission: "hi".into(),
            finish: Finish::Completed,
            resident: 12,
            capacity: 4096,
            request,
            measurement,
        }));
    });

    let session = SessionId("s-1".into());
    let sink = tempfile();
    let mut recorder = crate::record::Record::from(
        Recorder::receive(sink, RunRef("r-1".into()), SessionRef(session.0.clone()))
            .expect("recorder"),
    );
    let author = Author::new(&session, &weaver_types::RunId("r-1".into()));
    author
        .author(
            &mut recorder,
            Kind::Load,
            Subsystem::Harness,
            None,
            Some(Payload::Elections(Box::new(weaver_trace::Elections {
                residual_readout: false,
                field: Some(2),
                surprisal: false,
                tee: Some(weaver_trace::Election::default()),
                state_member: false,
                declaration: Default::default(),
                lineage: None,
                reset: None,
                stack: Default::default(),
                boundary: String::new(),
                cause: weaver_trace::Cause { uid: 0 },
                library_path: None,
                state_store: Default::default(),
                composer: weaver_trace::LoopIdentity::compiled("test"),
            }))),
        )
        .expect("load");
    let mut turn_ordinal = 0u64;
    let mut turn_in_flight: Option<weaver_types::TurnKey> = None;
    let (listener, _dir) = test_listener();
    let outcome = {
        let mut fullness = None;
        let mut pressure_reported = false;
        let load_facts = crate::engine::test_load_facts();
        let mut ports = Ports::grant(
            &decode,
            &author,
            &mut recorder,
            &mut turn_ordinal,
            &mut turn_in_flight,
            &load_facts,
            None,
            &listener,
            None,
            None,
            None,
            None,
            &mut fullness,
            &mut pressure_reported,
        );
        let delta = vec![Message {
            role: Role::User,
            content: vec![ContentBlock::Text { text: "hi".into() }],
        }];
        ports.turn(delta).expect("the turn completes")
    };
    peer.join().expect("the decode peer finishes");
    assert_eq!(outcome.emission, "hi", "the close still closes the turn");

    let fields: Vec<String> = recorder
        .structure()
        .expect("the serving record")
        .by_kind(Kind::ModelField)
        .map(|r| r.line.to_string())
        .collect();
    assert_eq!(fields.len(), 2, "one event per intermediate: {fields:?}");
    for (at, line) in fields.iter().enumerate() {
        let rendered: serde_json::Value =
            serde_json::from_str(line).expect("the line is one value");
        assert_eq!(
            rendered["payload"]["position"], at as u64,
            "the position the intermediate named: {line}"
        );
        assert_eq!(
            rendered["payload"]["ranked"][0]["token"], 11,
            "the ranking crosses whole: {line}"
        );
        assert_eq!(rendered["payload"]["realized"], 0, "and the realized rank");
        assert_eq!(
            rendered["subsystem"], "spu_decoder",
            "stamped where the field was produced: {line}"
        );
    }
}

/// **A fabricated tool result in loop 1's delta refuses before anything
/// authors**, per `weaver-harness-Spec` section 6: the role's one door is
/// the grant's, and a supplied result is a fabrication whatever it
/// carries. What this reads is the refusal arriving before the record
/// gains a single event of the turn's interior.
///
/// Perturbation: the door is doubled on purpose - `run_turn` refuses the
/// role and `author_message` refuses it again - so the watch drops both:
/// remove the role check at the top of `run_turn` and the refusal arm in
/// `author_message`, and this fails with `ChannelLost`, the fabricated
/// delta then authoring and the turn reaching a decode seam this test
/// deliberately closed. Watched under exactly that pair.
#[test]
fn a_supplied_tool_result_refuses_at_the_door() {
    let (near, far) = socketpair(
        AddressFamily::Unix,
        SockType::SeqPacket,
        None,
        SockFlag::SOCK_CLOEXEC,
    )
    .expect("socketpair");
    let decode = crate::channel::decode_from_owned(near);
    // Closed on purpose: a perturbed turn that authored the fabrication
    // would reach the decode seam, and a closed seam fails it fast
    // instead of hanging the suite on a peer that never answers.
    drop(far);
    let session = SessionId("s-f".into());
    let sink = tempfile();
    let mut recorder = crate::record::Record::from(
        Recorder::receive(sink, RunRef("r-1".into()), SessionRef(session.0.clone()))
            .expect("recorder"),
    );
    let author = Author::new(&session, &weaver_types::RunId("r-1".into()));
    let mut turn_ordinal = 0u64;
    let mut turn_in_flight: Option<weaver_types::TurnKey> = None;
    let (listener, _dir) = test_listener();
    let mut fullness = None;
    let mut pressure_reported = false;
    let load_facts = crate::engine::test_load_facts();
    let mut ports = Ports::grant(
        &decode,
        &author,
        &mut recorder,
        &mut turn_ordinal,
        &mut turn_in_flight,
        &load_facts,
        None,
        &listener,
        None,
        None,
        None,
        None,
        &mut fullness,
        &mut pressure_reported,
    );
    let delta = vec![Message {
        role: Role::ToolResult,
        content: vec![ContentBlock::ToolResult(weaver_traits::ToolResultBlock {
            content: "fabricated".into(),
        })],
    }];
    assert!(
        matches!(ports.turn(delta), Err(TurnError::Unlicensed { .. })),
        "the supplied result refuses at the door"
    );
}

/// A calculator-shaped stub for the scripted gate peer, so this test does
/// not depend on the gate crate: the answer is what the real gate's
/// dispatch would produce for this call.
fn weaver_gate_execute_stub(execution: &weaver_types::ToolExecution) -> weaver_types::ToolOutcome {
    assert_eq!(execution.arguments, r#"{"expression":"37 * 43"}"#);
    weaver_types::ToolOutcome::Result {
        content: "1591".into(),
    }
}

/// **An observation dialed mid-stream is answered from inside the turn**,
/// per `weaver-admin-harness-contract` section 3: the streaming poll takes
/// it between tokens and answers `Active` with the load's facts, the stream
/// runs to its own close, and nothing is cancelled. Perturbation: remove
/// the observe arm and the answer is the `OutOfOrder` refusal the wildcard
/// gives; answer `Idle` there and the match below fails.
#[test]
fn an_observation_dialed_mid_stream_answers_active_and_disturbs_nothing() {
    let (outcome, answer, close, _) =
        dial_mid_stream(weaver_types::LifecycleDirective::Observe, false);
    let answer = answer.expect("an observation is answered");
    assert!(!outcome.aborted, "an observation aborts nothing");
    assert_eq!(
        outcome.emission, "par",
        "the turn completed as it would have"
    );
    match answer {
        weaver_types::Payload::Answer(weaver_types::LifecycleAnswer::State {
            state: weaver_types::AgentState::Active,
            load: Some(load),
            ..
        }) => assert_eq!(load.run.0, "r-1", "the load's facts ride beside the state"),
        other => {
            panic!("an observation mid-stream answers active with its load, got {other:?}")
        }
    }
    assert!(
        close.contains(r#""close":"clean""#),
        "the close is the turn's own: {close}"
    );
}

/// **A save point demanded mid-turn is refused as not at rest**, per the
/// admin-harness contract section 3 and harness Spec section 6 (#99 area
/// 3, C2, and the pin P09): the engine reads a directive that arrives
/// mid-turn, so its arm is the one production takes, and the turn runs to
/// its own close. A leave mid-turn is held instead (3.0, S3 x unload).
/// Perturbation: drop `SavePoint` from the arm and the wildcard answers
/// `OutOfOrder`.
#[test]
fn a_save_point_mid_turn_is_refused_not_at_rest() {
    let cause = weaver_types::Cause { uid: 0 };
    {
        let directive = weaver_types::LifecycleDirective::SavePoint { cause };
        let named = format!("{directive:?}");
        let (outcome, answer, close, _) = dial_mid_stream(directive, false);
        let answer = answer.expect("a save point is answered");
        assert!(
            matches!(
                answer,
                weaver_types::Payload::Refusal(weaver_types::LifecycleRefusal::ActivityNotAtRest)
            ),
            "{named} mid-turn is refused not at rest, got {answer:?}"
        );
        assert!(!outcome.aborted, "{named} aborts nothing");
        assert!(
            close.contains(r#""close":"clean""#),
            "{named}: the turn closes as its own: {close}"
        );
    }
}

/// One turn streamed with `directive` already on the admin connection,
/// the way the poll's readiness would deliver it mid-stream: the turn's
/// outcome, the directive's answer, and the turn's close line.
fn dial_mid_stream(
    directive: weaver_types::LifecycleDirective,
    hold: bool,
) -> (
    TurnOutcome,
    Option<weaver_types::Payload>,
    String,
    Option<(crate::lifecycle_table::LeaveProgress, weaver_types::Cause)>,
) {
    let (near, far) = socketpair(
        AddressFamily::Unix,
        SockType::SeqPacket,
        None,
        SockFlag::SOCK_CLOEXEC,
    )
    .expect("socketpair");
    let decode = crate::channel::decode_from_owned(near);
    let (listener, _dir) = test_listener();
    let (verb_end, admin_end) = crate::channel::OrganChannel::pair().expect("pair");
    let admin = admin_end.into_channel();

    // The directive is already on the connection when the turn begins,
    // the way the poll's readiness would deliver it mid-stream.
    admin
        .send(&weaver_types::OrganEnvelope {
            exchange: weaver_types::ExchangeId {
                opener: weaver_types::Opener::Admin,
                ordinal: 9,
            },
            position: weaver_types::Position::Open,
            payload: weaver_types::Payload::Directive(directive),
        })
        .expect("the observe sends");

    let peer = std::thread::spawn(move || {
        let mut buf = vec![0u8; 65536];
        let n = recv(far.as_raw_fd(), &mut buf, MsgFlags::empty()).expect("recv append");
        let directive: weaver_types::TokenDirective =
            serde_json::from_slice(&buf[..n]).expect("the append parses");
        assert!(matches!(
            directive,
            weaver_types::TokenDirective::AppendAndGenerate { .. }
        ));
        // The cancel arrives because the wait heard the stop.
        let send_answer = |answer: &weaver_types::TokenAnswer| {
            let bytes = serde_json::to_vec(answer).expect("answer renders");
            send(far.as_raw_fd(), &bytes, MsgFlags::empty()).expect("send answer");
        };
        send_answer(&weaver_types::TokenAnswer::Token {
            token: 7,
            piece: "par".into(),
        });
        let request = serde_json::value::RawValue::from_string(
            r#"{"rendered":"r","template":"t","sampling":{}}"#.to_string(),
        )
        .unwrap();
        let measurement = serde_json::value::RawValue::from_string(
            r#"{"model":"m","weights_hash":"h","input_tokens":[1],"output_tokens":[7],"blocks":[],"timings":{"prefill_ns":"1","decode_ns":"2"}}"#
                .to_string(),
        )
        .unwrap();
        send_answer(&weaver_types::TokenAnswer::Generated(Generation {
            unrecovered: vec![],
            content: vec![],
            emission: "par".into(),
            finish: Finish::Completed,
            resident: 64,
            capacity: 4096,
            request,
            measurement,
        }));
        // The SPU stays standing until the turn is joined, as a real
        // one does, so a cancel the engine sends finds it.
        far
    });

    let session = SessionId("s-3".into());
    let sink = tempfile();
    let mut recorder = crate::record::Record::from(
        Recorder::receive(sink, RunRef("r-1".into()), SessionRef(session.0.clone()))
            .expect("recorder"),
    );
    let author = Author::new(&session, &weaver_types::RunId("r-1".into()));
    author
        .author(
            &mut recorder,
            Kind::Load,
            Subsystem::Harness,
            None,
            Some(Payload::Elections(Box::new(weaver_trace::Elections {
                residual_readout: false,
                field: None,
                surprisal: false,
                tee: Some(weaver_trace::Election::default()),
                state_member: false,
                declaration: Default::default(),
                lineage: None,
                reset: None,
                stack: Default::default(),
                boundary: String::new(),
                cause: weaver_trace::Cause { uid: 0 },
                library_path: None,
                state_store: Default::default(),
                composer: weaver_trace::LoopIdentity::compiled("test"),
            }))),
        )
        .expect("load");
    let mut turn_ordinal = 0u64;
    let mut turn_in_flight: Option<weaver_types::TurnKey> = None;
    let mut slot = Some(verb_end);
    let mut held: Option<PendingLeave> = None;

    let outcome = {
        let mut fullness = None;
        let mut pressure_reported = false;
        let load_facts = crate::engine::test_load_facts();
        let mut ports = Ports::grant(
            &decode,
            &author,
            &mut recorder,
            &mut turn_ordinal,
            &mut turn_in_flight,
            &load_facts,
            None,
            &listener,
            Some(&mut slot),
            None,
            None,
            None,
            &mut fullness,
            &mut pressure_reported,
        );
        let delta = vec![Message {
            role: Role::User,
            content: vec![ContentBlock::Text {
                text: "observe me".into(),
            }],
        }];
        if hold {
            ports.hold_leave(&mut held, None);
        }
        ports.turn(delta).expect("the observed turn returns")
    };
    peer.join().expect("the decode peer finishes");

    // A held leave keeps admin's connection; dropping it ends the
    // connection, so admin then reads the end where nothing was answered.
    let leave = held.take();
    let kept = leave.as_ref().map(|leave| (leave.progress, leave.cause));
    drop(leave);
    let answer = admin.recv().ok().map(|envelope| envelope.payload);
    let close = recorder
        .structure()
        .expect("the serving record")
        .by_kind(Kind::TurnClosed)
        .next()
        .expect("the close authored")
        .line
        .to_string();
    (outcome, answer, close, kept)
}

/// **A graceful leave heard mid-turn is held, and the turn finishes**
/// (`weaver-admin-Spec` 3.0, S3 x unload: S4, the turn continuing): the
/// engine takes admin's connection into the held leave and answers
/// nothing; the turn closes `Clean`, its own. Perturbation: refuse it
/// `ActivityNotAtRest` as before, and nothing is held.
#[test]
fn a_graceful_leave_mid_turn_is_held_and_the_turn_finishes() {
    let cause = weaver_types::Cause { uid: 7 };
    let (outcome, answer, close, held) = dial_mid_stream(
        weaver_types::LifecycleDirective::Leave {
            cause,
            forced: false,
            rollback: false,
        },
        true,
    );
    let (progress, held_cause) = held.expect("the leave is held");
    assert_eq!(
        progress,
        crate::lifecycle_table::LeaveProgress {
            directed: crate::lifecycle_table::Leave::Graceful,
            leave: crate::lifecycle_table::Leave::Graceful,
            step: crate::lifecycle_table::LeaveStep::Draining,
        }
    );
    assert_eq!(held_cause, cause);
    assert!(answer.is_none(), "nothing is answered mid-turn: {answer:?}");
    assert!(!outcome.aborted);
    assert!(close.contains(r#""close":"clean""#), "{close}");
}

/// **A forced leave heard mid-turn cancels the turn** (3.0, S3 x
/// force-unload: S10, the turn cancelled `Stopped { reason: unload }`
/// with the leave's cause), and the leave is held for the serve loop.
/// Perturbation: close the turn `Clean`, or name the directive as its
/// reason, and this fails.
#[test]
fn a_forced_leave_mid_turn_cancels_the_turn_stopped_by_unload() {
    let cause = weaver_types::Cause { uid: 7 };
    let (_, answer, close, held) = dial_mid_stream(
        weaver_types::LifecycleDirective::Leave {
            cause,
            forced: true,
            rollback: false,
        },
        true,
    );
    let (progress, _) = held.expect("the leave is held");
    assert_eq!(progress.directed, crate::lifecycle_table::Leave::Forced);
    assert!(answer.is_none(), "{answer:?}");
    assert!(close.contains(r#""reason":"unload""#), "{close}");
    assert!(close.contains(r#""uid":7"#), "the leave's cause: {close}");
}

/// What a turn with a leave heard during it left behind: its outcome,
/// the tool completion and the close as authored, the interrupts and
/// executions the gate saw, the appends the SPU saw, and the held leave.
struct LeaveDuringTurn {
    outcome: TurnOutcome,
    completed: serde_json::Value,
    close: serde_json::Value,
    interrupts: usize,
    executions: usize,
    appends: usize,
    held: Option<crate::lifecycle_table::LeaveProgress>,
}

/// One turn whose generation asks one tool call, with a leave on admin's
/// connection. `before_call` delays the generation so the leave is heard
/// on the decode stream, before the call is asked; otherwise the leave is
/// heard during the call's execution. `crossing` has the gate send its
/// result before it reads the interrupt. `granted` is the gate's shell
/// grant (#29).
fn leave_during_turn(
    forced: bool,
    crossing: bool,
    before_call: bool,
    granted: bool,
) -> LeaveDuringTurn {
    use nix::poll::{PollFd, PollFlags, poll};
    use std::os::fd::AsFd;
    use std::sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    };
    use std::time::{Duration, Instant};
    let (listener, _dir) = test_listener();
    let (near, far) = socketpair(
        AddressFamily::Unix,
        SockType::SeqPacket,
        None,
        SockFlag::SOCK_CLOEXEC,
    )
    .unwrap();
    let decode = crate::channel::decode_from_owned(near);
    let (verb_end, admin_end) = crate::channel::OrganChannel::pair().unwrap();
    let admin = std::sync::Arc::new(admin_end.into_channel());
    let leave = weaver_types::OrganEnvelope {
        exchange: weaver_types::ExchangeId {
            opener: weaver_types::Opener::Admin,
            ordinal: 9,
        },
        position: weaver_types::Position::Open,
        payload: weaver_types::Payload::Directive(weaver_types::LifecycleDirective::Leave {
            cause: weaver_types::Cause { uid: 7 },
            forced,
            rollback: false,
        }),
    };
    let (gate_near, gate_child) = crate::channel::OrganChannel::pair().unwrap();
    let generated = |content, finish| {
        weaver_types::TokenAnswer::Generated(Generation {
            unrecovered: vec![],
            content,
            emission: "tool emission".into(),
            finish,
            resident: 64,
            capacity: 4096,
            request: serde_json::value::RawValue::from_string(r#"{"rendered":"fixture"}"#.into())
                .unwrap(),
            measurement: serde_json::value::RawValue::from_string(r#"{}"#.into()).unwrap(),
        })
    };
    let with_call = generated(
        vec![ContentBlock::ToolCall(weaver_traits::ToolCall {
            name: "sh".into(),
            arguments: r#"{"command":"sleep 5"}"#.into(),
        })],
        Finish::Completed,
    );
    if !before_call {
        send(
            far.as_raw_fd(),
            &serde_json::to_vec(&with_call).unwrap(),
            MsgFlags::empty(),
        )
        .unwrap();
    }
    let finished = Arc::new(AtomicBool::new(false));
    let decode_finished = finished.clone();
    let final_answer = generated(vec![], Finish::Completed);
    let stopped = generated(vec![], Finish::Stopped);
    let peer = std::thread::spawn(move || {
        let mut appends = 0;
        let until = Instant::now() + Duration::from_secs(6);
        loop {
            let mut fds = [PollFd::new(far.as_fd(), PollFlags::POLLIN)];
            if poll(&mut fds, 25u16).unwrap() > 0 {
                let mut buf = vec![0; 65536];
                let n = recv(far.as_raw_fd(), &mut buf, MsgFlags::empty()).unwrap();
                if n == 0 {
                    break;
                }
                let request: weaver_types::TokenDirective =
                    serde_json::from_slice(&buf[..n]).unwrap();
                let reply = match request {
                    TokenDirective::AppendAndGenerate { .. } => {
                        appends += 1;
                        if before_call && appends == 1 {
                            // The leave is heard on the stream first.
                            std::thread::sleep(Duration::from_millis(150));
                            &with_call
                        } else {
                            &final_answer
                        }
                    }
                    TokenDirective::Cancel { .. } => &stopped,
                    _ => continue,
                };
                // The first generation was queued before the turn began.
                if appends != 1 || before_call || matches!(request, TokenDirective::Cancel { .. }) {
                    send(
                        far.as_raw_fd(),
                        &serde_json::to_vec(reply).unwrap(),
                        MsgFlags::empty(),
                    )
                    .unwrap();
                }
            } else if decode_finished.load(Ordering::Acquire) {
                break;
            }
            assert!(Instant::now() < until, "decode fixture did not finish");
        }
        appends
    });
    let gate_finished = finished.clone();
    // **The leave arrives during the call** where the case says so, sent
    // once the gate holds the execution: the turn's wait scans the verb
    // connection before its work, so a leave sent before the turn is
    // heard during the generation, the before-the-call case.
    let (leaver, during_call) = (admin.clone(), leave.clone());
    let gate_peer = std::thread::spawn(move || {
        let gate = gate_child.into_channel();
        let (mut interrupts, mut executions) = (0, 0);
        let until = Instant::now() + Duration::from_secs(6);
        loop {
            let mut fds = [PollFd::new(gate.as_fd(), PollFlags::POLLIN)];
            if poll(&mut fds, 25u16).unwrap() > 0 {
                let Ok(envelope) = gate.recv() else {
                    break;
                };
                match envelope.payload {
                    weaver_types::Payload::Tool(_) => {
                        executions += 1;
                        if !before_call {
                            leaver.send(&during_call).unwrap();
                        }
                        let answer = |outcome| {
                            gate.send(&weaver_types::OrganEnvelope {
                                exchange: envelope.exchange.clone(),
                                position: weaver_types::Position::Close,
                                payload: weaver_types::Payload::ToolAnswer(outcome),
                            })
                            .unwrap();
                        };
                        if crossing {
                            answer(weaver_types::ToolOutcome::Result {
                                content: "finished as the interrupt crossed".into(),
                            });
                        }
                        let mut fds = [PollFd::new(gate.as_fd(), PollFlags::POLLIN)];
                        if poll(&mut fds, 2_000u16).unwrap() > 0 {
                            let interrupt = gate.recv().unwrap();
                            assert_eq!(interrupt.exchange, envelope.exchange);
                            assert_eq!(interrupt.position, weaver_types::Position::Continue);
                            assert_eq!(
                                interrupt.payload,
                                weaver_types::Payload::ToolInterrupt {
                                    by: weaver_types::KillCause::Unload
                                }
                            );
                            interrupts += 1;
                            if !crossing {
                                answer(weaver_types::ToolOutcome::Killed {
                                    partial: Some("partial".into()),
                                    by: weaver_types::KillCause::Unload,
                                });
                            }
                        } else if !crossing {
                            answer(weaver_types::ToolOutcome::Result {
                                content: "no interrupt came".into(),
                            });
                        }
                    }
                    weaver_types::Payload::ToolInterrupt {
                        by: weaver_types::KillCause::Unload,
                    } => interrupts += 1,
                    other => panic!("unexpected gate traffic: {other:?}"),
                }
            } else if gate_finished.load(Ordering::Acquire) {
                break;
            }
            assert!(Instant::now() < until, "gate fixture did not finish");
        }
        (interrupts, executions)
    });
    let session = SessionId("s-leave-during-turn".into());
    let mut recorder = crate::record::Record::from(
        Recorder::receive(
            tempfile(),
            RunRef("r-1".into()),
            SessionRef(session.0.clone()),
        )
        .expect("recorder"),
    );
    let author = Author::new(&session, &weaver_types::RunId("r-1".into()));
    author
        .author(
            &mut recorder,
            Kind::Load,
            Subsystem::Harness,
            None,
            Some(Payload::Elections(Box::new(weaver_trace::Elections {
                residual_readout: false,
                field: None,
                surprisal: false,
                tee: Some(weaver_trace::Election::default()),
                state_member: false,
                declaration: Default::default(),
                lineage: None,
                reset: None,
                stack: Default::default(),
                boundary: String::new(),
                cause: weaver_trace::Cause { uid: 0 },
                library_path: None,
                state_store: Default::default(),
                composer: weaver_trace::LoopIdentity::compiled("test"),
            }))),
        )
        .expect("load");
    let (mut turn_ordinal, mut turn_in_flight, mut slot) = (0, None, Some(verb_end));
    let (mut gate_ordinal, mut held_frames) = (0, std::collections::VecDeque::new());
    let (mut fullness, mut pressure_reported) = (None, false);
    let mut held: Option<PendingLeave> = None;
    let load_facts = test_load_facts();
    if before_call {
        admin.send(&leave).unwrap();
    }
    let result = {
        let mut ports = Ports::grant(
            &decode,
            &author,
            &mut recorder,
            &mut turn_ordinal,
            &mut turn_in_flight,
            &load_facts,
            None,
            &listener,
            Some(&mut slot),
            Some(GatePort {
                channel: &gate_near,
                ordinal: &mut gate_ordinal,
                held: &mut held_frames,
                tools: None,
                contained: None,
                shell_granted: granted,
            }),
            None,
            None,
            &mut fullness,
            &mut pressure_reported,
        );
        ports.hold_leave(&mut held, None);
        ports.turn(vec![Message {
            role: Role::User,
            content: vec![ContentBlock::Text {
                text: "leave during the turn".into(),
            }],
        }])
    };
    finished.store(true, Ordering::Release);
    let appends = peer.join().unwrap();
    let (interrupts, executions) = gate_peer.join().unwrap();
    let trace = recorder.structure().unwrap();
    let event = |kind| {
        serde_json::from_str::<serde_json::Value>(
            &trace.by_kind(kind).next().expect("trace event").line,
        )
        .unwrap()
    };
    LeaveDuringTurn {
        outcome: result.expect("turn returns"),
        completed: event(Kind::ToolCallCompleted),
        close: event(Kind::TurnClosed),
        interrupts,
        executions,
        appends,
        held: held.map(|held| held.progress),
    }
}

/// **A stop, a second leave or a save point meeting a held leave is
/// `OutOfOrder`** (`weaver-harness-Spec` section 6: a `Leave` meeting a
/// pending leave is `OutOfOrder`, as is a `SavePoint`; the self-check of
/// B2): the turn goes on uncancelled, its close the leave's, and the
/// answer agrees with what the leave's sweeps answer at rest.
/// Perturbation: admit the stop as before, and the generation is
/// cancelled while the close reads clean.
/// One turn with a graceful leave held, against a generation that
/// answers after `directive` is heard on the turn's connection. Answers
/// whether the turn finished unaborted, and the dialer's answer.
fn a_directive_mid_turn_with_a_leave_held(
    directive: weaver_types::LifecycleDirective,
) -> (bool, weaver_types::Payload) {
    let (listener, _dir) = test_listener();
    let (near, far) = socketpair(
        AddressFamily::Unix,
        SockType::SeqPacket,
        None,
        SockFlag::SOCK_CLOEXEC,
    )
    .unwrap();
    let decode = crate::channel::decode_from_owned(near);
    // The generation answers after the directive is heard.
    let peer = std::thread::spawn(move || {
        let mut buf = vec![0; 65536];
        let n = recv(far.as_raw_fd(), &mut buf, MsgFlags::empty()).unwrap();
        let _: TokenDirective = serde_json::from_slice(&buf[..n]).unwrap();
        std::thread::sleep(std::time::Duration::from_millis(300));
        let answer = weaver_types::TokenAnswer::Generated(Generation {
            unrecovered: vec![],
            content: vec![],
            emission: "done".into(),
            finish: Finish::Completed,
            resident: 64,
            capacity: 4096,
            request: serde_json::value::RawValue::from_string(r#"{"rendered":"r"}"#.into())
                .unwrap(),
            measurement: serde_json::value::RawValue::from_string("{}".into()).unwrap(),
        });
        send(
            far.as_raw_fd(),
            &serde_json::to_vec(&answer).unwrap(),
            MsgFlags::empty(),
        )
        .unwrap();
        far
    });
    let (verb_end, dialer) = crate::channel::OrganChannel::pair().unwrap();
    let dialer = dialer.into_channel();
    dialer
        .send(&weaver_types::OrganEnvelope {
            exchange: weaver_types::ExchangeId {
                opener: weaver_types::Opener::Admin,
                ordinal: 1,
            },
            position: weaver_types::Position::Open,
            payload: weaver_types::Payload::Directive(directive.clone()),
        })
        .unwrap();
    let (connection, _admin) = crate::channel::OrganChannel::pair().unwrap();
    let mut held = Some(PendingLeave {
        connection,
        exchange: weaver_types::ExchangeId {
            opener: weaver_types::Opener::Admin,
            ordinal: 4,
        },
        cause: weaver_types::Cause { uid: 7 },
        rollback: false,
        forced_by: None,
        progress: crate::lifecycle_table::LeaveProgress {
            directed: crate::lifecycle_table::Leave::Graceful,
            leave: crate::lifecycle_table::Leave::Graceful,
            step: crate::lifecycle_table::LeaveStep::Draining,
        },
        drain_deadline: None,
        joins: Vec::new(),
    });
    let mut slot = Some(verb_end);
    let (mut recorder, author) = loaded_recorder();
    let (mut turn_ordinal, mut turn_in_flight) = (0, None);
    let (mut fullness, mut pressure_reported) = (None, false);
    let load_facts = test_load_facts();
    let outcome = {
        let mut ports = Ports::grant(
            &decode,
            &author,
            &mut recorder,
            &mut turn_ordinal,
            &mut turn_in_flight,
            &load_facts,
            None,
            &listener,
            Some(&mut slot),
            None,
            None,
            None,
            &mut fullness,
            &mut pressure_reported,
        );
        ports.hold_leave(&mut held, None);
        ports.turn(vec![Message {
            role: Role::User,
            content: vec![ContentBlock::Text {
                text: "a turn".into(),
            }],
        }])
    };
    let _far = peer.join().unwrap();
    let finished = outcome.is_ok_and(|outcome| !outcome.aborted);
    (finished, dialer.recv().expect("an answer").payload)
}

#[test]
fn a_directive_meeting_a_held_leave_mid_turn_is_out_of_order() {
    for directive in [
        weaver_types::LifecycleDirective::Stop {
            cause: weaver_types::Cause { uid: 9 },
        },
        weaver_types::LifecycleDirective::Leave {
            cause: weaver_types::Cause { uid: 9 },
            forced: false,
            rollback: false,
        },
        weaver_types::LifecycleDirective::SavePoint {
            cause: weaver_types::Cause { uid: 9 },
        },
    ] {
        let (finished, answer) = a_directive_mid_turn_with_a_leave_held(directive.clone());
        assert!(finished, "{directive:?}: the turn goes on");
        assert_eq!(
            answer,
            weaver_types::Payload::Refusal(weaver_types::LifecycleRefusal::OutOfOrder),
            "{directive:?}"
        );
    }
}

/// **An observation mid-turn with a leave held answers `InTransition`**
/// (`weaver-types-Spec` section 3.1; `weaver-harness-Spec` section 8; the
/// Planner's grade of #119), never `Active`: the run is between states.
/// Perturbation: answer the held-leave observation as an idle turn's, and
/// it reads `State`.
#[test]
fn an_observation_mid_turn_with_a_leave_held_answers_in_transition() {
    let (finished, answer) =
        a_directive_mid_turn_with_a_leave_held(weaver_types::LifecycleDirective::Observe);
    assert!(finished);
    assert_eq!(
        answer,
        weaver_types::Payload::Answer(weaver_types::LifecycleAnswer::InTransition {
            constituents: Vec::new(),
        })
    );
}

/// **A join heard during the wind-down cancels its generation and turns
/// the leave forced, naming `forced_by`** (`weaver-admin-Spec` 3.0, S5 x
/// force-unload): the joiner is held for the same `Left`, and the
/// wind-down closes `Stopped { reason: unload }` with the leave's cause.
/// Perturbation: refuse the join `OutOfOrder` mid-turn, and the
/// generation answers late and complete.
#[test]
fn a_join_during_the_wind_down_cancels_it_and_turns_the_leave_forced() {
    use nix::poll::{PollFd, PollFlags, poll};
    use std::os::fd::AsFd;
    let (listener, _dir) = test_listener();
    let (near, far) = socketpair(
        AddressFamily::Unix,
        SockType::SeqPacket,
        None,
        SockFlag::SOCK_CLOEXEC,
    )
    .unwrap();
    let decode = crate::channel::decode_from_owned(near);
    let peer = std::thread::spawn(move || {
        let mut buf = vec![0; 65536];
        let n = recv(far.as_raw_fd(), &mut buf, MsgFlags::empty()).unwrap();
        let _: TokenDirective = serde_json::from_slice(&buf[..n]).unwrap();
        let mut fds = [PollFd::new(far.as_fd(), PollFlags::POLLIN)];
        let cancelled = poll(&mut fds, 3_000u16).unwrap() > 0
            && recv(far.as_raw_fd(), &mut buf, MsgFlags::empty()).is_ok_and(|n| {
                matches!(
                    serde_json::from_slice(&buf[..n]),
                    Ok(TokenDirective::Cancel { .. })
                )
            });
        let answer = weaver_types::TokenAnswer::Generated(Generation {
            unrecovered: vec![],
            content: vec![],
            emission: "summary".into(),
            finish: if cancelled {
                Finish::Stopped
            } else {
                Finish::Completed
            },
            resident: 64,
            capacity: 4096,
            request: serde_json::value::RawValue::from_string(r#"{"rendered":"r"}"#.into())
                .unwrap(),
            measurement: serde_json::value::RawValue::from_string("{}".into()).unwrap(),
        });
        send(
            far.as_raw_fd(),
            &serde_json::to_vec(&answer).unwrap(),
            MsgFlags::empty(),
        )
        .unwrap();
        (cancelled, far)
    });
    // The leave's own dialer, and a join dialed during the wind-down.
    let (connection, _admin) = crate::channel::OrganChannel::pair().unwrap();
    let (join_end, joiner) = crate::channel::OrganChannel::pair().unwrap();
    let joiner = joiner.into_channel();
    joiner
        .send(&weaver_types::OrganEnvelope {
            exchange: weaver_types::ExchangeId {
                opener: weaver_types::Opener::Admin,
                ordinal: 1,
            },
            position: weaver_types::Position::Open,
            payload: weaver_types::Payload::Directive(
                weaver_types::LifecycleDirective::JoinLeave {
                    cause: weaver_types::Cause { uid: 9 },
                },
            ),
        })
        .unwrap();
    let mut slot = Some(join_end);
    let mut held = Some(PendingLeave {
        connection,
        exchange: weaver_types::ExchangeId {
            opener: weaver_types::Opener::Admin,
            ordinal: 4,
        },
        cause: weaver_types::Cause { uid: 7 },
        rollback: false,
        forced_by: None,
        progress: crate::lifecycle_table::LeaveProgress {
            directed: crate::lifecycle_table::Leave::Graceful,
            leave: crate::lifecycle_table::Leave::Graceful,
            step: crate::lifecycle_table::LeaveStep::Draining,
        },
        drain_deadline: None,
        joins: Vec::new(),
    });
    let (mut recorder, author) = loaded_recorder();
    let (mut turn_ordinal, mut turn_in_flight) = (0, None);
    let (mut fullness, mut pressure_reported) = (None, false);
    let load_facts = test_load_facts();
    let (ran, cut) = {
        let mut ports = Ports::grant(
            &decode,
            &author,
            &mut recorder,
            &mut turn_ordinal,
            &mut turn_in_flight,
            &load_facts,
            None,
            &listener,
            Some(&mut slot),
            None,
            None,
            None,
            &mut fullness,
            &mut pressure_reported,
        );
        ports.hold_leave(&mut held, None);
        ports.wind_down(None, weaver_types::Cause { uid: 7 })
    };
    let (cancelled, _far) = peer.join().unwrap();
    assert!(cancelled, "the wind-down's generation is cancelled");
    assert!(ran.is_ok() && cut, "{ran:?}");
    let held = held.expect("the leave");
    assert_eq!(held.progress.leave, crate::lifecycle_table::Leave::Forced);
    assert_eq!(held.forced_by, Some(weaver_types::Cause { uid: 9 }));
    assert_eq!(held.joins.len(), 1, "the joiner held for the same Left");
    let close = serde_json::from_str::<serde_json::Value>(
        &recorder
            .structure()
            .unwrap()
            .by_kind(Kind::TurnClosed)
            .next()
            .expect("the close")
            .line,
    )
    .unwrap();
    assert_eq!(close["payload"]["reason"], "unload", "{close}");
    assert_eq!(close["payload"]["cause"]["uid"], 7, "{close}");
}

/// **A leave is heard while the SPU streams** (Codex on #119, round 3;
/// 3.0, S3 x force-unload): the turn's wait scans the lifecycle wakes
/// (the verb connection, the listener, the gate's hang-up) before the
/// SPU's work, so a generation that keeps its channel readable cannot
/// starve a forced `Leave` dialed mid-turn. The leave is held, the turn
/// cancelled. Perturbation: scan the work first, and the stream runs its
/// 3 s with the leave unread.
#[test]
fn a_leave_dialed_while_the_spu_streams_is_heard() {
    use nix::poll::{PollFd, PollFlags, poll};
    use std::os::fd::AsFd;
    let (listener, _dir) = test_listener();
    let (near, far) = socketpair(
        AddressFamily::Unix,
        SockType::SeqPacket,
        None,
        SockFlag::SOCK_CLOEXEC,
    )
    .unwrap();
    let decode = crate::channel::decode_from_owned(near);
    let (verb_end, admin_end) = crate::channel::OrganChannel::pair().unwrap();
    let admin = admin_end.into_channel();
    let peer = std::thread::spawn(move || {
        let mut buf = vec![0; 65536];
        let n = recv(far.as_raw_fd(), &mut buf, MsgFlags::empty()).unwrap();
        let _: TokenDirective = serde_json::from_slice(&buf[..n]).unwrap();
        let started = std::time::Instant::now();
        let token = serde_json::to_vec(&weaver_types::TokenAnswer::Token {
            token: 1,
            piece: "a".into(),
        })
        .unwrap();
        let cancelled = loop {
            let mut fds = [PollFd::new(far.as_fd(), PollFlags::POLLIN)];
            if poll(&mut fds, 0u16).unwrap() > 0 {
                let n = recv(far.as_raw_fd(), &mut buf, MsgFlags::empty()).unwrap();
                if matches!(
                    serde_json::from_slice(&buf[..n]),
                    Ok(TokenDirective::Cancel { .. })
                ) {
                    break true;
                }
            }
            if started.elapsed() > std::time::Duration::from_secs(3) {
                break false;
            }
            while send(far.as_raw_fd(), &token, MsgFlags::MSG_DONTWAIT).is_ok() {}
        };
        let answer = weaver_types::TokenAnswer::Generated(Generation {
            unrecovered: vec![],
            content: vec![],
            emission: "streamed".into(),
            finish: if cancelled {
                Finish::Stopped
            } else {
                Finish::Completed
            },
            resident: 64,
            capacity: 4096,
            request: serde_json::value::RawValue::from_string(r#"{"rendered":"r"}"#.into())
                .unwrap(),
            measurement: serde_json::value::RawValue::from_string("{}".into()).unwrap(),
        });
        send(
            far.as_raw_fd(),
            &serde_json::to_vec(&answer).unwrap(),
            MsgFlags::empty(),
        )
        .unwrap();
        (cancelled, started.elapsed(), far)
    });
    // The forced leave, sent once the stream is flowing.
    std::thread::spawn(move || {
        std::thread::sleep(std::time::Duration::from_millis(200));
        admin
            .send(&weaver_types::OrganEnvelope {
                exchange: weaver_types::ExchangeId {
                    opener: weaver_types::Opener::Admin,
                    ordinal: 9,
                },
                position: weaver_types::Position::Open,
                payload: weaver_types::Payload::Directive(
                    weaver_types::LifecycleDirective::Leave {
                        cause: weaver_types::Cause { uid: 7 },
                        forced: true,
                        rollback: false,
                    },
                ),
            })
            .unwrap();
        admin
    });
    let (mut recorder, author) = loaded_recorder();
    let (mut turn_ordinal, mut turn_in_flight, mut slot) = (0, None, Some(verb_end));
    let (mut fullness, mut pressure_reported) = (None, false);
    let mut held: Option<PendingLeave> = None;
    let load_facts = test_load_facts();
    let outcome = {
        let mut ports = Ports::grant(
            &decode,
            &author,
            &mut recorder,
            &mut turn_ordinal,
            &mut turn_in_flight,
            &load_facts,
            None,
            &listener,
            Some(&mut slot),
            None,
            None,
            None,
            &mut fullness,
            &mut pressure_reported,
        );
        ports.hold_leave(&mut held, None);
        ports.turn(vec![Message {
            role: Role::User,
            content: vec![ContentBlock::Text {
                text: "a long stream".into(),
            }],
        }])
    };
    let (cancelled, took, _far) = peer.join().unwrap();
    assert!(outcome.is_ok(), "{outcome:?}");
    assert!(
        cancelled,
        "the forced leave cancels the stream, after {took:?}"
    );
    assert!(took < std::time::Duration::from_secs(2), "{took:?}");
    assert_eq!(
        held.expect("the leave is held").progress.directed,
        crate::lifecycle_table::Leave::Forced
    );
}

/// **A bound passes while the SPU streams** (Codex on #119, round 1): a
/// generation whose tokens keep its channel readable is the one a
/// declared bound exists to cut, so the deadline is read on every wake,
/// not only when the wait times out. The SPU keeps its channel full of
/// tokens past the drain's 1 s bound, and is cancelled at it. Perturbation: read
/// the deadline only on a timed-out wait, and the stream runs its 3 s.
#[test]
fn a_bound_passing_while_the_spu_streams_cancels_the_turn() {
    use nix::poll::{PollFd, PollFlags, poll};
    use std::os::fd::AsFd;
    let (listener, _dir) = test_listener();
    let (near, far) = socketpair(
        AddressFamily::Unix,
        SockType::SeqPacket,
        None,
        SockFlag::SOCK_CLOEXEC,
    )
    .unwrap();
    let decode = crate::channel::decode_from_owned(near);
    let (verb_end, admin_end) = crate::channel::OrganChannel::pair().unwrap();
    let admin = admin_end.into_channel();
    let peer = std::thread::spawn(move || {
        let mut buf = vec![0; 65536];
        let n = recv(far.as_raw_fd(), &mut buf, MsgFlags::empty()).unwrap();
        let _: TokenDirective = serde_json::from_slice(&buf[..n]).unwrap();
        let started = std::time::Instant::now();
        let token = serde_json::to_vec(&weaver_types::TokenAnswer::Token {
            token: 1,
            piece: "a".into(),
        })
        .unwrap();
        let cancelled = loop {
            let mut fds = [PollFd::new(far.as_fd(), PollFlags::POLLIN)];
            if poll(&mut fds, 0u16).unwrap() > 0 {
                let n = recv(far.as_raw_fd(), &mut buf, MsgFlags::empty()).unwrap();
                if matches!(
                    serde_json::from_slice(&buf[..n]),
                    Ok(TokenDirective::Cancel { .. })
                ) {
                    break true;
                }
            }
            if started.elapsed() > std::time::Duration::from_secs(3) {
                break false;
            }
            // The channel kept full: a token always waits to be read.
            while send(far.as_raw_fd(), &token, MsgFlags::MSG_DONTWAIT).is_ok() {}
        };
        let answer = weaver_types::TokenAnswer::Generated(Generation {
            unrecovered: vec![],
            content: vec![],
            emission: "streamed".into(),
            finish: if cancelled {
                Finish::Stopped
            } else {
                Finish::Completed
            },
            resident: 64,
            capacity: 4096,
            request: serde_json::value::RawValue::from_string(r#"{"rendered":"r"}"#.into())
                .unwrap(),
            measurement: serde_json::value::RawValue::from_string("{}".into()).unwrap(),
        });
        send(
            far.as_raw_fd(),
            &serde_json::to_vec(&answer).unwrap(),
            MsgFlags::empty(),
        )
        .unwrap();
        (cancelled, started.elapsed(), far)
    });
    admin
        .send(&weaver_types::OrganEnvelope {
            exchange: weaver_types::ExchangeId {
                opener: weaver_types::Opener::Admin,
                ordinal: 9,
            },
            position: weaver_types::Position::Open,
            payload: weaver_types::Payload::Directive(weaver_types::LifecycleDirective::Leave {
                cause: weaver_types::Cause { uid: 7 },
                forced: false,
                rollback: false,
            }),
        })
        .unwrap();
    let (mut recorder, author) = loaded_recorder();
    let (mut turn_ordinal, mut turn_in_flight, mut slot) = (0, None, Some(verb_end));
    let (mut fullness, mut pressure_reported) = (None, false);
    let mut held: Option<PendingLeave> = None;
    let load_facts = test_load_facts();
    let outcome = {
        let mut ports = Ports::grant(
            &decode,
            &author,
            &mut recorder,
            &mut turn_ordinal,
            &mut turn_in_flight,
            &load_facts,
            None,
            &listener,
            Some(&mut slot),
            None,
            None,
            None,
            &mut fullness,
            &mut pressure_reported,
        );
        ports.hold_leave(&mut held, Some(1));
        ports.turn(vec![Message {
            role: Role::User,
            content: vec![ContentBlock::Text {
                text: "a long stream".into(),
            }],
        }])
    };
    let (cancelled, took, _far) = peer.join().unwrap();
    assert!(outcome.is_ok(), "{outcome:?}");
    assert!(
        cancelled,
        "the stream is cancelled at the bound, after {took:?}"
    );
    assert!(took < std::time::Duration::from_secs(2), "{took:?}");
    assert_eq!(
        held.expect("the leave is held").progress.leave,
        crate::lifecycle_table::Leave::Forced
    );
}

/// **A declared drain bound passing mid-turn cancels the turn and turns
/// the held leave forced** (`weaver-admin-Spec` 3.0, S4 x a bound passes):
/// the generation is cancelled as a stop's is, the turn closes
/// `Stopped { reason: unload }` with the leave's cause, and the held
/// leave reads forced, so it goes to the lower with no wind-down.
/// Perturbation: keep the wait unbounded, and the generation answers
/// late and complete, the leave still graceful.
#[test]
fn a_drain_bound_passing_mid_turn_cancels_the_turn_and_turns_the_leave_forced() {
    use nix::poll::{PollFd, PollFlags, poll};
    use std::os::fd::AsFd;
    let (listener, _dir) = test_listener();
    let (near, far) = socketpair(
        AddressFamily::Unix,
        SockType::SeqPacket,
        None,
        SockFlag::SOCK_CLOEXEC,
    )
    .unwrap();
    let decode = crate::channel::decode_from_owned(near);
    let (verb_end, admin_end) = crate::channel::OrganChannel::pair().unwrap();
    let admin = admin_end.into_channel();
    // The generation answers late, 3 s after its append, or at once
    // stopped where a cancel arrives first.
    let peer = std::thread::spawn(move || {
        let mut buf = vec![0; 65536];
        let n = recv(far.as_raw_fd(), &mut buf, MsgFlags::empty()).unwrap();
        let _: TokenDirective = serde_json::from_slice(&buf[..n]).unwrap();
        let mut fds = [PollFd::new(far.as_fd(), PollFlags::POLLIN)];
        let cancelled = poll(&mut fds, 3_000u16).unwrap() > 0
            && recv(far.as_raw_fd(), &mut buf, MsgFlags::empty()).is_ok_and(|n| {
                matches!(
                    serde_json::from_slice(&buf[..n]),
                    Ok(TokenDirective::Cancel { .. })
                )
            });
        let answer = weaver_types::TokenAnswer::Generated(Generation {
            unrecovered: vec![],
            content: vec![],
            emission: "late".into(),
            finish: if cancelled {
                Finish::Stopped
            } else {
                Finish::Completed
            },
            resident: 64,
            capacity: 4096,
            request: serde_json::value::RawValue::from_string(r#"{"rendered":"r"}"#.into())
                .unwrap(),
            measurement: serde_json::value::RawValue::from_string("{}".into()).unwrap(),
        });
        send(
            far.as_raw_fd(),
            &serde_json::to_vec(&answer).unwrap(),
            MsgFlags::empty(),
        )
        .unwrap();
        (cancelled, far)
    });
    admin
        .send(&weaver_types::OrganEnvelope {
            exchange: weaver_types::ExchangeId {
                opener: weaver_types::Opener::Admin,
                ordinal: 9,
            },
            position: weaver_types::Position::Open,
            payload: weaver_types::Payload::Directive(weaver_types::LifecycleDirective::Leave {
                cause: weaver_types::Cause { uid: 7 },
                forced: false,
                rollback: false,
            }),
        })
        .unwrap();
    let (mut recorder, author) = loaded_recorder();
    let (mut turn_ordinal, mut turn_in_flight, mut slot) = (0, None, Some(verb_end));
    let (mut fullness, mut pressure_reported) = (None, false);
    let mut held: Option<PendingLeave> = None;
    let load_facts = test_load_facts();
    let outcome = {
        let mut ports = Ports::grant(
            &decode,
            &author,
            &mut recorder,
            &mut turn_ordinal,
            &mut turn_in_flight,
            &load_facts,
            None,
            &listener,
            Some(&mut slot),
            None,
            None,
            None,
            &mut fullness,
            &mut pressure_reported,
        );
        ports.hold_leave(&mut held, Some(1));
        ports.turn(vec![Message {
            role: Role::User,
            content: vec![ContentBlock::Text {
                text: "a long turn".into(),
            }],
        }])
    };
    let (cancelled, _far) = peer.join().unwrap();
    assert!(cancelled, "the generation is cancelled at the bound");
    assert!(outcome.is_ok(), "{outcome:?}");
    let close = serde_json::from_str::<serde_json::Value>(
        &recorder
            .structure()
            .unwrap()
            .by_kind(Kind::TurnClosed)
            .next()
            .expect("the close")
            .line,
    )
    .unwrap();
    assert_eq!(close["payload"]["reason"], "unload", "{close}");
    assert_eq!(close["payload"]["cause"]["uid"], 7, "{close}");
    let held = held.expect("the leave is held");
    assert_eq!(held.progress.leave, crate::lifecycle_table::Leave::Forced);
    assert_eq!(
        held.progress.directed,
        crate::lifecycle_table::Leave::Graceful
    );
}

/// **A call out when the leave is heard is interrupted, killed by the
/// unload, and its turn closes stopped** (`weaver-admin-Spec` 3.0, S4 x
/// tool return and x turn closes): the gate is sent `ToolInterrupt`, never
/// a cancel; the call completes `Killed { by: unload }`, never re-fed;
/// and the turn closes `Stopped { reason: unload }` with the leave's
/// cause. Perturbation: re-feed the killed call, or close the turn clean.
#[test]
fn a_call_out_when_the_leave_is_heard_is_interrupted_and_its_turn_closes_stopped() {
    let run = leave_during_turn(false, false, false, true);
    assert_eq!((run.interrupts, run.executions), (1, 1));
    assert_eq!(
        run.completed["payload"]["by"], "unload",
        "{}",
        run.completed
    );
    assert_eq!(run.appends, 1, "the killed call is never re-fed");
    assert_eq!(run.close["payload"]["reason"], "unload", "{}", run.close);
    assert_eq!(run.close["payload"]["cause"]["uid"], 7, "{}", run.close);
    assert!(run.held.is_some());
}

/// **A result that crossed the interrupt stands** (3.0, S4 x tool return,
/// the first outcome wins): it completes with its result, is delivered to
/// the turn as in S3, and the turn, having finished, closes `Clean`.
/// Perturbation: count a crossed result as the unload's, and the turn
/// closes stopped.
#[test]
fn a_crossed_result_is_delivered_and_its_turn_closes_clean() {
    let run = leave_during_turn(false, true, false, true);
    assert_eq!(run.executions, 1);
    assert_eq!(
        run.completed["payload"]["outcome"], "result",
        "{}",
        run.completed
    );
    assert_eq!(run.appends, 2, "the result is delivered to the turn");
    assert_eq!(run.close["payload"]["close"], "clean", "{}", run.close);
    assert!(!run.outcome.aborted);
}

/// **A call asked after the leave was heard is never sent** (3.0, S4 x
/// tool return): the gate sees no execution, the call completes killed by
/// the unload in this crate's own word, and the turn closes stopped by the
/// unload. Perturbation: send it, and the gate executes it.
#[test]
fn a_call_asked_after_the_leave_is_never_sent() {
    let run = leave_during_turn(false, false, true, true);
    assert_eq!(run.executions, 0, "never sent");
    assert_eq!(run.completed["subsystem"], "harness", "{}", run.completed);
    assert_eq!(
        run.completed["payload"]["by"], "unload",
        "{}",
        run.completed
    );
    assert_eq!(run.close["payload"]["reason"], "unload", "{}", run.close);
}

/// **An ungranted call asked after the leave was heard reads `Refused`,
/// never killed by the unload** (#29): it could never have run, so the
/// harness's refusal is judged before the leave's branch wherever a gate
/// stands. Nothing is sent and the turn still closes stopped by the unload.
/// Perturbation: judge the grant after the leave's branch and the
/// completion reads `Killed { by: unload }`.
#[test]
fn an_ungranted_call_after_the_leave_reads_refused() {
    let run = leave_during_turn(false, false, true, false);
    assert_eq!(run.executions, 0, "never sent");
    assert_eq!(run.completed["subsystem"], "harness", "{}", run.completed);
    assert!(
        run.completed["payload"]["reason"]
            .as_str()
            .is_some_and(|reason| reason.contains("is not a tool")),
        "{}",
        run.completed
    );
    assert!(
        run.completed["payload"]["by"].is_null(),
        "{}",
        run.completed
    );
}

/// **The wind-down sends no call** (`weaver-admin-Spec` 3.0, graceful
/// step 3): with the gate drained, a call the model asks in the wind-down
/// turn completes killed by the unload in this crate's word, recorded
/// interrupted, and the turn closes clean to S6 (S5 x tool return and x
/// turn closes). Perturbation: drop the `winding_down` flag, and the call
/// stands unexecuted with no completion.
#[test]
fn a_call_asked_in_the_wind_down_is_never_sent() {
    let (near, far) = socketpair(
        AddressFamily::Unix,
        SockType::SeqPacket,
        None,
        SockFlag::SOCK_CLOEXEC,
    )
    .expect("socketpair");
    let asked = weaver_types::TokenAnswer::Generated(Generation {
        unrecovered: vec![],
        content: vec![ContentBlock::ToolCall(weaver_traits::ToolCall {
            name: "sh".into(),
            arguments: r#"{"command":"true"}"#.into(),
        })],
        emission: "a call".into(),
        finish: Finish::Completed,
        resident: 64,
        capacity: 4096,
        request: serde_json::value::RawValue::from_string(r#"{"rendered":"fixture"}"#.into())
            .unwrap(),
        measurement: serde_json::value::RawValue::from_string(r#"{}"#.into()).unwrap(),
    });
    send(
        far.as_raw_fd(),
        &serde_json::to_vec(&asked).unwrap(),
        MsgFlags::empty(),
    )
    .unwrap();
    let decode = crate::channel::decode_from_owned(near);
    let (mut recorder, author) = loaded_recorder();
    let mut turn_ordinal = 3u64;
    let mut turn_in_flight = None;
    let (listener, _dir) = test_listener();
    let mut fullness = None;
    let mut pressure_reported = false;
    let load_facts = crate::engine::test_load_facts();
    let ran = Ports::grant(
        &decode,
        &author,
        &mut recorder,
        &mut turn_ordinal,
        &mut turn_in_flight,
        &load_facts,
        None,
        &listener,
        None,
        None,
        None,
        None,
        &mut fullness,
        &mut pressure_reported,
    )
    .wind_down(None, weaver_types::Cause { uid: 7 })
    .0;
    assert!(ran.is_ok(), "{ran:?}");
    let lines: Vec<serde_json::Value> = recorder
        .structure()
        .expect("the serving record")
        .iter()
        .filter(|r| matches!(r.kind, Kind::ToolCallCompleted | Kind::TurnClosed))
        .map(|r| serde_json::from_str(r.line.as_ref()).unwrap())
        .collect();
    let completed = lines
        .iter()
        .find(|line| line["kind"] == "tool.call.completed")
        .unwrap_or_else(|| panic!("the call completes: {lines:?}"));
    assert_eq!(completed["subsystem"], "harness", "{completed}");
    assert_eq!(completed["payload"]["by"], "unload", "{completed}");
    let close = lines
        .iter()
        .find(|line| line["kind"] == "turn.closed")
        .unwrap_or_else(|| panic!("the turn closes: {lines:?}"));
    assert_eq!(close["payload"]["close"], "clean", "{close}");
}

/// **A forced leave heard during a call interrupts it and cancels the
/// turn** (3.0, S10 x tool return and x turn closes). Perturbation: send
/// a cancel rather than the interrupt, and the gate fixture fails on it.
#[test]
fn a_forced_leave_during_a_call_interrupts_it_and_cancels_the_turn() {
    let run = leave_during_turn(true, false, false, true);
    assert_eq!(run.interrupts, 1);
    assert_eq!(
        run.completed["payload"]["by"], "unload",
        "{}",
        run.completed
    );
    assert_eq!(run.appends, 1);
    assert_eq!(run.close["payload"]["reason"], "unload", "{}", run.close);
    assert_eq!(
        run.held.map(|held| held.directed),
        Some(crate::lifecycle_table::Leave::Forced)
    );
}

/// conforms: harness-stop-polled-during-the-invocation
/// The gate releases its delay only for a correlated cancel. Removing
/// coordination from the wait fails the delay and completion assertions.
#[test]
fn a_stop_during_an_invocation_cancels_and_records_before_acknowledging() {
    stopped_invocation(false, 1);
    stopped_invocation(true, 1);
}

/// conforms: harness-stopped-invocation-feeds-no-generation
/// Removing the stopped check after completion opens the second call
/// and feeds the result to decode, both observed on the real pairs.
#[test]
fn a_stopped_invocation_feeds_nothing_and_opens_no_further_call() {
    stopped_invocation(false, 2);
}

fn stopped_invocation(crossing: bool, call_count: usize) {
    use nix::poll::{PollFd, PollFlags, poll};
    use std::os::fd::AsFd;
    use std::sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    };
    use std::time::{Duration, Instant};
    const DELAY_MS: u16 = 2_000;
    let (listener, _dir) = test_listener();
    let (near, far) = socketpair(
        AddressFamily::Unix,
        SockType::SeqPacket,
        None,
        SockFlag::SOCK_CLOEXEC,
    )
    .unwrap();
    let decode = crate::channel::decode_from_owned(near);
    let (verb_end, admin_end) = crate::channel::OrganChannel::pair().unwrap();
    let admin = Arc::new(admin_end.into_channel());
    let stop_exchange = weaver_types::ExchangeId {
        opener: weaver_types::Opener::Admin,
        ordinal: 9,
    };
    let (gate_near, gate_child) = crate::channel::OrganChannel::pair().unwrap();
    let generated = |content, finish| {
        weaver_types::TokenAnswer::Generated(Generation {
            unrecovered: vec![],
            content,
            emission: "tool emission".into(),
            finish,
            resident: 64,
            capacity: 4096,
            request: serde_json::value::RawValue::from_string(r#"{"rendered":"fixture"}"#.into())
                .unwrap(),
            measurement: serde_json::value::RawValue::from_string(r#"{}"#.into()).unwrap(),
        })
    };
    let calls = (0..call_count)
        .map(|_| {
            ContentBlock::ToolCall(weaver_traits::ToolCall {
                name: "sh".into(),
                arguments: r#"{"command":"sleep 5"}"#.into(),
            })
        })
        .collect();
    // Both are ready before the turn: decode's initial answer wins, then
    // the invocation wait must hear coordination while gate holds its answer.
    send(
        far.as_raw_fd(),
        &serde_json::to_vec(&generated(calls, Finish::Completed)).unwrap(),
        MsgFlags::empty(),
    )
    .unwrap();
    let finished = Arc::new(AtomicBool::new(false));
    let decode_finished = finished.clone();
    let stopped = generated(vec![], Finish::Stopped);
    let peer = std::thread::spawn(move || {
        let mut appends = 0;
        let until = Instant::now() + Duration::from_secs(6);
        loop {
            let mut fds = [PollFd::new(far.as_fd(), PollFlags::POLLIN)];
            let ready = poll(&mut fds, 25u16).unwrap();
            if ready > 0 {
                let mut buf = vec![0; 65536];
                let n = recv(far.as_raw_fd(), &mut buf, MsgFlags::empty()).unwrap();
                if n == 0 {
                    break;
                }
                let request: weaver_types::TokenDirective =
                    serde_json::from_slice(&buf[..n]).unwrap();
                if matches!(request, TokenDirective::AppendAndGenerate { .. }) {
                    appends += 1;
                }
                if appends > 1 || matches!(request, TokenDirective::Cancel { .. }) {
                    send(
                        far.as_raw_fd(),
                        &serde_json::to_vec(&stopped).unwrap(),
                        MsgFlags::empty(),
                    )
                    .unwrap();
                }
            } else if decode_finished.load(Ordering::Acquire) {
                break;
            }
            assert!(Instant::now() < until, "decode fixture did not finish");
        }
        appends
    });
    let gate_finished = finished.clone();
    // **The stop is dialed during the execution**, once the gate holds
    // it: the turn's wait scans the verb connection before its work, so
    // a stop sent before the turn would be heard during the generation,
    // and the invocation's own stop is what this fixture drives.
    let (stopper, stop_sent) = (admin.clone(), stop_exchange.clone());
    let gate_peer = std::thread::spawn(move || {
        let gate = gate_child.into_channel();
        let mut fds = [PollFd::new(gate.as_fd(), PollFlags::POLLIN)];
        assert!(poll(&mut fds, 5_000u16).unwrap() > 0);
        let open = gate.recv().unwrap();
        assert_eq!(open.position, weaver_types::Position::Open);
        assert!(matches!(open.payload, weaver_types::Payload::Tool(_)));
        stopper
            .send(&weaver_types::OrganEnvelope {
                exchange: stop_sent,
                position: weaver_types::Position::Open,
                payload: weaver_types::Payload::Directive(weaver_types::LifecycleDirective::Stop {
                    cause: weaver_types::Cause { uid: 0 },
                }),
            })
            .unwrap();
        let mut fds = [PollFd::new(gate.as_fd(), PollFlags::POLLIN)];
        let ready = poll(&mut fds, DELAY_MS).unwrap() > 0;
        let answer = |outcome| {
            gate.send(&weaver_types::OrganEnvelope {
                exchange: open.exchange.clone(),
                position: weaver_types::Position::Close,
                payload: weaver_types::Payload::ToolAnswer(outcome),
            })
            .unwrap();
        };
        // In the crossing arm the result leaves before the gate reads
        // the queued cancel, as on a naturally completed invocation.
        if crossing {
            answer(weaver_types::ToolOutcome::Result {
                content: "finished as cancel crossed".into(),
            });
        }
        let mut cancels = 0;
        if ready {
            let cancel = gate.recv().unwrap();
            assert_eq!(cancel.exchange, open.exchange);
            assert_eq!(cancel.position, weaver_types::Position::Continue);
            assert_eq!(cancel.payload, weaver_types::Payload::ToolCancel);
            cancels += 1;
        }
        if !crossing {
            answer(if ready {
                weaver_types::ToolOutcome::Killed {
                    partial: Some("partial".into()),
                    by: weaver_types::KillCause::Cancel,
                }
            } else {
                weaver_types::ToolOutcome::Result {
                    content: "finished at fixture delay".into(),
                }
            });
        }
        let mut executions = 1;
        let until = Instant::now() + Duration::from_secs(6);
        loop {
            let mut fds = [PollFd::new(gate.as_fd(), PollFlags::POLLIN)];
            if poll(&mut fds, 25u16).unwrap() > 0 {
                let Ok(envelope) = gate.recv() else {
                    break;
                };
                match envelope.payload {
                    weaver_types::Payload::ToolCancel => cancels += 1,
                    weaver_types::Payload::Tool(_) => {
                        executions += 1;
                        gate.send(&weaver_types::OrganEnvelope {
                            exchange: envelope.exchange,
                            position: weaver_types::Position::Close,
                            payload: weaver_types::Payload::ToolAnswer(
                                weaver_types::ToolOutcome::Result {
                                    content: "unwanted second execution".into(),
                                },
                            ),
                        })
                        .unwrap();
                    }
                    _ => panic!("unexpected gate traffic"),
                }
            } else if gate_finished.load(Ordering::Acquire) {
                break;
            }
            assert!(Instant::now() < until, "gate fixture did not finish");
        }
        (cancels, executions)
    });
    let session = SessionId("s-stop-invocation".into());
    let mut recorder = crate::record::Record::from(
        Recorder::receive(
            tempfile(),
            RunRef("r-1".into()),
            SessionRef(session.0.clone()),
        )
        .expect("recorder"),
    );
    // Author owns its origin. Bracket its construction so trace
    // monotonic timestamps map to intervals on this test's clock
    // without changing production code or treating return as close.
    let origin_before = Instant::now();
    let author = Author::new(&session, &weaver_types::RunId("r-1".into()));
    let origin_after = Instant::now();
    author
        .author(
            &mut recorder,
            Kind::Load,
            Subsystem::Harness,
            None,
            Some(Payload::Elections(Box::new(weaver_trace::Elections {
                residual_readout: false,
                field: None,
                surprisal: false,
                tee: Some(weaver_trace::Election::default()),
                state_member: false,
                declaration: Default::default(),
                lineage: None,
                reset: None,
                stack: Default::default(),
                boundary: String::new(),
                cause: weaver_trace::Cause { uid: 0 },
                library_path: None,
                state_store: Default::default(),
                composer: weaver_trace::LoopIdentity::compiled("test"),
            }))),
        )
        .expect("load");
    let mut turn_ordinal = 0;
    let mut turn_in_flight = None;
    let mut slot = Some(verb_end);
    let mut gate_ordinal = 0;
    let mut held = std::collections::VecDeque::new();
    let mut fullness = None;
    let mut pressure_reported = false;
    let load_facts = test_load_facts();

    let stop_before = Instant::now();
    // Observe acknowledgment on its own thread, so an early send cannot
    // hide in the socket buffer until after turn() has returned.
    let acknowledgment = std::thread::spawn(move || {
        let mut fds = [PollFd::new(admin.as_fd(), PollFlags::POLLIN)];
        assert!(poll(&mut fds, 5_000u16).unwrap() > 0, "stop unanswered");
        let answer = admin.recv().unwrap();
        (answer, Instant::now())
    });
    let result = {
        let mut ports = Ports::grant(
            &decode,
            &author,
            &mut recorder,
            &mut turn_ordinal,
            &mut turn_in_flight,
            &load_facts,
            None,
            &listener,
            Some(&mut slot),
            Some(GatePort {
                channel: &gate_near,
                ordinal: &mut gate_ordinal,
                held: &mut held,
                tools: None,
                contained: None,
                shell_granted: true,
            }),
            None,
            None,
            &mut fullness,
            &mut pressure_reported,
        );
        ports.turn(vec![Message {
            role: Role::User,
            content: vec![ContentBlock::Text {
                text: "stop invocation".into(),
            }],
        }])
    };
    finished.store(true, Ordering::Release);
    let appends = peer.join().unwrap();
    let (cancels, executions) = gate_peer.join().unwrap();
    let (answer, acknowledged) = acknowledgment.join().unwrap();
    let outcome = result.expect("turn returns");
    assert!(outcome.aborted && outcome.stopped);
    assert_eq!(answer.exchange, stop_exchange);
    assert!(matches!(
        answer.payload,
        weaver_types::Payload::Answer(weaver_types::LifecycleAnswer::TurnAborted { .. })
    ));
    assert_eq!(cancels, 1, "exactly one cancel");
    assert_eq!(executions, 1, "no second execution");
    assert_eq!(appends, 1, "no result re-feed");
    let trace = recorder.structure().unwrap();
    let event = |kind| {
        serde_json::from_str::<serde_json::Value>(
            &trace.by_kind(kind).next().expect("trace event").line,
        )
        .unwrap()
    };
    let completed = event(Kind::ToolCallCompleted);
    let close = event(Kind::TurnClosed);
    let ns = |event: &serde_json::Value| {
        event["monotonic_ns"]
            .as_str()
            .unwrap()
            .parse::<u64>()
            .unwrap()
    };
    assert!(ns(&completed) < ns(&close), "completion precedes close");
    assert!(
        origin_after + Duration::from_nanos(ns(&close)) <= acknowledged,
        "record precedes acknowledgment"
    );
    assert!(
        origin_before + Duration::from_nanos(ns(&close)) < stop_before + Duration::from_millis(800),
        "stop spent fixture delay"
    );
    assert!(
        close.to_string().contains(r#""reason":"directive""#),
        "{close}"
    );
    if crossing {
        assert!(
            completed.to_string().contains("finished as cancel crossed"),
            "{completed}"
        );
    } else {
        assert!(
            completed.to_string().contains(r#""by":"cancel""#),
            "{completed}"
        );
    }
}

/// **The stop is heard mid-stream, per Spec 6.1.** The streaming wait
/// spans the decode channel and the verb connection at once, the stop
/// cancels the turn at the seam, the partial stands in the close with
/// the directive's reason, and the stop is answered with the turn's
/// fate after the record. Perturbation: collapse the streaming wait to
/// the decode channel alone and the cancel this scripted peer waits
/// for never arrives.
#[test]
fn a_stop_dialed_mid_stream_cancels_the_turn() {
    stop_dialed_mid_stream(false);
}

/// **A directive mid-turn is not answered over a latched record** (#116):
/// the record is latched by a submission refused on its own (an event bound
/// to another session, `RefusedOnSubmit`), the writer sound. The stop's
/// wake reads the latch before anything is answered, the turn ends on the
/// loss with its one close landed, and no answer reaches the dialer.
/// Perturbation: drop the read from the directive wake, and the turn runs
/// on to the stop's cancel and returns `Ok`.
#[test]
fn a_directive_mid_turn_is_not_answered_over_a_latched_record() {
    stop_dialed_mid_stream(true);
}

fn stop_dialed_mid_stream(latched: bool) {
    let (near, far) = socketpair(
        AddressFamily::Unix,
        SockType::SeqPacket,
        None,
        SockFlag::SOCK_CLOEXEC,
    )
    .expect("socketpair");
    let decode = crate::channel::decode_from_owned(near);
    let (listener, _dir) = test_listener();
    let (verb_end, admin_end) = crate::channel::OrganChannel::pair().expect("pair");
    let admin = admin_end.into_channel();

    // The operator's stop is already on the connection when the turn
    // begins, the way the poll's readiness would deliver it mid-stream.
    admin
        .send(&weaver_types::OrganEnvelope {
            exchange: weaver_types::ExchangeId {
                opener: weaver_types::Opener::Admin,
                ordinal: 9,
            },
            position: weaver_types::Position::Open,
            payload: weaver_types::Payload::Directive(weaver_types::LifecycleDirective::Stop {
                cause: weaver_types::Cause { uid: 0 },
            }),
        })
        .expect("the stop sends");

    // **The peer holds its end until the turn has returned**, so a turn
    // that ends here ends on the latch and not on the decode channel's
    // hang-up; released, or past two seconds, it answers as the unlatched
    // run's peer does.
    let (release, released) = std::sync::mpsc::channel::<()>();
    let peer = std::thread::spawn(move || {
        let mut buf = vec![0u8; 65536];
        let n = recv(far.as_raw_fd(), &mut buf, MsgFlags::empty()).expect("recv append");
        let directive: weaver_types::TokenDirective =
            serde_json::from_slice(&buf[..n]).expect("the append parses");
        assert!(matches!(
            directive,
            weaver_types::TokenDirective::AppendAndGenerate { .. }
        ));
        if latched
            && released
                .recv_timeout(std::time::Duration::from_secs(2))
                .is_ok()
        {
            return;
        }
        // The cancel arrives because the wait heard the stop.
        let n = recv(far.as_raw_fd(), &mut buf, MsgFlags::empty()).expect("recv cancel");
        let directive: weaver_types::TokenDirective =
            serde_json::from_slice(&buf[..n]).expect("the cancel parses");
        assert!(
            matches!(directive, weaver_types::TokenDirective::Cancel { .. }),
            "the stop cancels at the seam"
        );
        let send_answer = |answer: &weaver_types::TokenAnswer| {
            let bytes = serde_json::to_vec(answer).expect("answer renders");
            send(far.as_raw_fd(), &bytes, MsgFlags::empty()).expect("send answer");
        };
        send_answer(&weaver_types::TokenAnswer::Token {
            token: 7,
            piece: "par".into(),
        });
        let request = serde_json::value::RawValue::from_string(
            r#"{"rendered":"r","template":"t","sampling":{}}"#.to_string(),
        )
        .unwrap();
        let measurement = serde_json::value::RawValue::from_string(
            r#"{"model":"m","weights_hash":"h","input_tokens":[1],"output_tokens":[7],"blocks":[],"timings":{"prefill_ns":"1","decode_ns":"2"}}"#
                .to_string(),
        )
        .unwrap();
        send_answer(&weaver_types::TokenAnswer::Generated(Generation {
            unrecovered: vec![],
            content: vec![],
            emission: "par".into(),
            finish: Finish::Stopped,
            resident: 64,
            capacity: 4096,
            request,
            measurement,
        }));
    });

    let session = SessionId("s-2".into());
    let sink = tempfile();
    let mut recorder = crate::record::Record::from(
        Recorder::receive(sink, RunRef("r-1".into()), SessionRef(session.0.clone()))
            .expect("recorder"),
    );
    let author = Author::new(&session, &weaver_types::RunId("r-1".into()));
    author
        .author(
            &mut recorder,
            Kind::Load,
            Subsystem::Harness,
            None,
            Some(Payload::Elections(Box::new(weaver_trace::Elections {
                residual_readout: false,
                field: None,
                surprisal: false,
                tee: Some(weaver_trace::Election::default()),
                state_member: false,
                declaration: Default::default(),
                lineage: None,
                reset: None,
                stack: Default::default(),
                boundary: String::new(),
                cause: weaver_trace::Cause { uid: 0 },
                library_path: None,
                state_store: Default::default(),
                composer: weaver_trace::LoopIdentity::compiled("test"),
            }))),
        )
        .expect("load");
    if latched {
        let foreign = Author::new(
            &SessionId("s-other".into()),
            &weaver_types::RunId("r-1".into()),
        );
        let refused = foreign.author_fault(
            &mut recorder,
            Subsystem::Harness,
            None,
            &crate::authorship::harness_report(weaver_types::FaultCase::OrganDeathObserved, "{}"),
        );
        assert!(
            matches!(
                refused,
                Err(crate::record::RecordFailure::Serving(
                    weaver_trace::Failure::RefusedOnSubmit { .. }
                ))
            ),
            "{refused:?}"
        );
        assert!(recorder.failed(), "the refusal is latched");
    }
    let mut turn_ordinal = 0u64;
    let mut turn_in_flight: Option<weaver_types::TurnKey> = None;
    let mut slot = Some(verb_end);

    let outcome = {
        let mut fullness = None;
        let mut pressure_reported = false;
        let load_facts = crate::engine::test_load_facts();
        let mut ports = Ports::grant(
            &decode,
            &author,
            &mut recorder,
            &mut turn_ordinal,
            &mut turn_in_flight,
            &load_facts,
            None,
            &listener,
            Some(&mut slot),
            None,
            None,
            None,
            &mut fullness,
            &mut pressure_reported,
        );
        let delta = vec![Message {
            role: Role::User,
            content: vec![ContentBlock::Text {
                text: "stop me".into(),
            }],
        }];
        ports.turn(delta)
    };
    let _ = release.send(());
    peer.join().expect("the decode peer finishes");

    if latched {
        assert!(
            matches!(outcome, Err(TurnError::ChannelLost)),
            "the turn ends on the latch: {outcome:?}"
        );
    } else {
        let outcome = outcome.expect("the aborted turn still returns");
        assert!(outcome.aborted, "the directive aborted the turn");
        assert_eq!(outcome.emission, "par", "the partial stands");
    }

    if latched {
        use std::os::fd::AsFd;
        let mut fds = [nix::poll::PollFd::new(
            admin.as_fd(),
            nix::poll::PollFlags::POLLIN,
        )];
        let ready = nix::poll::poll(&mut fds, nix::poll::PollTimeout::from(200u16)).expect("poll");
        assert_eq!(ready, 0, "no answer is announced over a latched record");
    } else {
        // Announce after record: the answer carries the turn's fate.
        match admin.recv().expect("the stop's answer").payload {
            weaver_types::Payload::Answer(weaver_types::LifecycleAnswer::TurnAborted { turn }) => {
                assert_eq!(turn.0, "t-1");
            }
            other => panic!("the stop answers the turn's fate, got {other:?}"),
        }
    }

    // The close names the directive, the partial standing before it.
    let close = recorder
        .structure()
        .expect("the serving record")
        .by_kind(Kind::TurnClosed)
        .next()
        .expect("the close authored")
        .line
        .to_string();
    if latched {
        assert!(
            close.contains(r#""close":"stopped""#) && close.contains(r#""reason":"fault""#),
            "the one close names the loss: {close}"
        );
    } else {
        assert!(
            close.contains(r#""close":"stopped""#) && close.contains(r#""reason":"directive""#),
            "the close names the directive, not the fault: {close}"
        );
    }
}

/// **A stop heard before the latch is set is not answered after the close**
/// (#116, `close_turn`'s announcement): the close lands, the writer sound,
/// and the latch is read before `TurnAborted` is sent. Driven at the close
/// directly, since no fixture sets the latch between the stop's wake and
/// the close. Perturbation: drop the read from `close_turn`, and the stop
/// is answered.
#[test]
fn a_stop_is_not_answered_after_a_close_over_a_latched_record() {
    let (decode_near, _decode_far) = socketpair(
        AddressFamily::Unix,
        SockType::SeqPacket,
        None,
        SockFlag::SOCK_CLOEXEC,
    )
    .expect("socketpair");
    let decode = crate::channel::decode_from_owned(decode_near);
    let (listener, _dir) = test_listener();
    let (verb_end, admin_end) = crate::channel::OrganChannel::pair().expect("pair");
    let admin = admin_end.into_channel();
    let session = SessionId("s-2".into());
    let mut recorder = crate::record::Record::from(
        Recorder::receive(
            tempfile(),
            RunRef("r-1".into()),
            SessionRef(session.0.clone()),
        )
        .expect("recorder"),
    );
    let author = Author::new(&session, &weaver_types::RunId("r-1".into()));
    let foreign = Author::new(
        &SessionId("s-other".into()),
        &weaver_types::RunId("r-1".into()),
    );
    assert!(
        foreign
            .author_fault(
                &mut recorder,
                Subsystem::Harness,
                None,
                &crate::authorship::harness_report(
                    weaver_types::FaultCase::OrganDeathObserved,
                    "{}"
                ),
            )
            .is_err()
    );
    assert!(recorder.failed(), "the refusal is latched");
    let turn = weaver_types::TurnKey("t-1".into());
    let mut turn_ordinal = 1u64;
    let mut turn_in_flight = Some(turn.clone());
    let mut slot = Some(verb_end);
    {
        let mut fullness = None;
        let mut pressure_reported = false;
        let load_facts = crate::engine::test_load_facts();
        let mut ports = Ports::grant(
            &decode,
            &author,
            &mut recorder,
            &mut turn_ordinal,
            &mut turn_in_flight,
            &load_facts,
            None,
            &listener,
            Some(&mut slot),
            None,
            None,
            None,
            &mut fullness,
            &mut pressure_reported,
        );
        let generation = Generation {
            unrecovered: vec![],
            content: vec![],
            emission: "par".into(),
            finish: Finish::Stopped,
            resident: 64,
            capacity: 4096,
            request: serde_json::value::RawValue::from_string("{}".into()).unwrap(),
            measurement: serde_json::value::RawValue::from_string("{}".into()).unwrap(),
        };
        let mut stop = Some((
            weaver_types::ExchangeId {
                opener: weaver_types::Opener::Admin,
                ordinal: 9,
            },
            weaver_types::Cause { uid: 0 },
        ));
        let aborted = ports
            .close_turn(&turn, &generation, &mut stop)
            .expect("the close lands");
        assert!(aborted, "the stop aborted the turn");
    }
    use std::os::fd::AsFd;
    let mut fds = [nix::poll::PollFd::new(
        admin.as_fd(),
        nix::poll::PollFlags::POLLIN,
    )];
    assert_eq!(
        nix::poll::poll(&mut fds, nix::poll::PollTimeout::from(200u16)).expect("poll"),
        0,
        "no answer is announced over a latched record"
    );
    assert!(
        recorder
            .structure()
            .expect("the serving record")
            .by_kind(Kind::TurnClosed)
            .next()
            .is_some(),
        "the close landed"
    );
}

/// **A column frame on the generate stream authors rather than kills.**
/// The contract's third intermediate rides the append-and-generate
/// exchange wherever the open's ask stood, so the scripted peer sends
/// one mid-stream against a diagnostic record and the turn completes
/// with the `residual.column` event authored where a wildcard would
/// have read channel loss.
///
/// Perturbation: remove the `Column` arm from the generate stream and
/// this fails, the turn dying of a frame the contract licenses.
/// Watched under exactly that removal.
#[test]
fn a_column_on_the_generate_stream_authors_rather_than_kills() {
    let (near, far) = socketpair(
        AddressFamily::Unix,
        SockType::SeqPacket,
        None,
        SockFlag::SOCK_CLOEXEC,
    )
    .expect("socketpair");
    let decode = crate::channel::decode_from_owned(near);

    let peer = std::thread::spawn(move || {
        let mut buf = vec![0u8; 65536];
        let _ = recv(far.as_raw_fd(), &mut buf, MsgFlags::empty()).expect("recv append");
        let column = r#"{"kind":"column","body":{"position":7,"layers":[[1.0,2.0],[3.0,4.0]]}}"#;
        send(far.as_raw_fd(), column.as_bytes(), MsgFlags::empty()).expect("send column");
        let generated = concat!(
            r#"{"kind":"generated","body":{"emission":"hi","finish":"completed","#,
            r#""content":[{"type":"text","text":"hi"}],"#,
            r#""request":{"rendered":"hi","template":"t","sampling":{}},"#,
            r#""measurement":{"input_tokens":[1],"output_tokens":[2]},"#,
            r#""resident":4,"capacity":64}}"#
        );
        send(far.as_raw_fd(), generated.as_bytes(), MsgFlags::empty()).expect("send close");
    });

    let session = SessionId("s-d".into());
    let sink_path = crate::scratch::Scratch(std::env::temp_dir().join(format!(
        "weaver-engine-column-{}-{:?}.ndjson",
        std::process::id(),
        std::thread::current().id()
    )));
    let sink = OwnedFd::from(std::fs::File::create(&sink_path).expect("sink"));
    let mut recorder = crate::record::Record::from(
        weaver_diagnostic::Recorder::receive(
            sink,
            weaver_diagnostic::RunRef("r-d".into()),
            weaver_diagnostic::SessionRef(session.0.clone()),
        )
        .expect("the recorder receives"),
    );
    let author = Author::new(&session, &weaver_types::RunId("r-d".into()));
    let mut turn_ordinal = 0u64;
    let mut turn_in_flight: Option<weaver_types::TurnKey> = None;
    let (listener, _dir) = test_listener();
    let outcome = {
        let mut fullness = None;
        let mut pressure_reported = false;
        let load_facts = crate::engine::test_load_facts();
        let mut ports = Ports::grant(
            &decode,
            &author,
            &mut recorder,
            &mut turn_ordinal,
            &mut turn_in_flight,
            &load_facts,
            None,
            &listener,
            None,
            None,
            None,
            None,
            &mut fullness,
            &mut pressure_reported,
        );
        ports.turn(vec![Message {
            role: Role::User,
            content: vec![ContentBlock::Text { text: "hi".into() }],
        }])
    };
    peer.join().expect("the decode peer finishes");
    assert!(outcome.is_ok(), "the turn completes: {outcome:?}");

    let mut text = String::new();
    use std::io::Read as _;
    std::fs::File::open(&sink_path)
        .expect("the record reopens")
        .read_to_string(&mut text)
        .expect("the record reads");
    let column = text
        .lines()
        .map(|l| serde_json::from_str::<serde_json::Value>(l).expect("a line parses"))
        .find(|e| e["kind"] == "residual.column")
        .expect("the column authored");
    assert_eq!(column["payload"]["position"], 7);
    assert_eq!(column["payload"]["layers"], 2);
    assert_eq!(column["payload"]["width"], 2);
    std::fs::remove_file(&sink_path).ok();
}

/// A serving recorder over a scratch sink, its load authored, the
/// shape every seat test below opens on.
/// Runs `ask` against a granted seat with `turn` standing, and answers
/// what it answered beside every `score` line the record then holds.
fn with_seat<R>(
    turn: Option<weaver_types::TurnKey>,
    ask: impl FnOnce(&mut Ports<'_>) -> R,
) -> (R, Vec<String>) {
    let (near, _far) = socketpair(
        AddressFamily::Unix,
        SockType::SeqPacket,
        None,
        SockFlag::SOCK_CLOEXEC,
    )
    .expect("socketpair");
    let decode = crate::channel::decode_from_owned(near);
    let (mut recorder, author) = loaded_recorder();
    let mut turn_ordinal = 3u64;
    let mut turn_in_flight = turn;
    let (listener, _dir) = test_listener();
    let mut fullness = None;
    let mut pressure_reported = false;
    let load_facts = crate::engine::test_load_facts();
    let answered = {
        let mut ports = Ports::grant(
            &decode,
            &author,
            &mut recorder,
            &mut turn_ordinal,
            &mut turn_in_flight,
            &load_facts,
            None,
            &listener,
            None,
            None,
            None,
            None,
            &mut fullness,
            &mut pressure_reported,
        );
        ask(&mut ports)
    };
    let scores = recorder
        .structure()
        .expect("the serving record")
        .iter()
        .filter(|r| r.kind == Kind::Score)
        .map(|r| r.line.to_string())
        .collect();
    (answered, scores)
}

/// **A run records one score, between turns, with its ratio's terms**
/// (#523). The first verdict lands and the port answers true, the
/// payload carrying the predicate, whether it held, and both terms. A
/// second verdict for the same run is refused and authors nothing.
///
/// Perturbation: drop the check for a score already standing in
/// `Ports::score` and the second verdict lands too, two lines where one
/// is asserted.
///
/// conforms: trace-score-records-the-verdict-and-its-terms
#[test]
fn a_run_records_one_score_between_turns() {
    let (answers, scores) = with_seat(None, |ports| {
        (
            ports.score("reached-the-goal", true, Some(14), Some(11)),
            ports.score("reached-the-goal", false, None, None),
        )
    });
    assert_eq!(
        answers,
        (true, false),
        "the first lands, the second refuses"
    );
    assert_eq!(scores.len(), 1, "one run, one verdict: {scores:?}");
    assert!(
        scores[0].contains(concat!(
            r#""payload":{"predicate":"reached-the-goal","passed":true,"#,
            r#""ratio":{"measured":14,"denominator":11}}"#
        )) && !scores[0].contains(r#""turn""#),
        "the verdict and both terms, turnless: {}",
        scores[0]
    );
}

/// **A score out of place or malformed is refused and authors nothing**
/// (#523): inside a standing turn, with one term of the ratio and not the
/// other, with a zero denominator, or with an empty predicate. A verdict
/// whose task supplies no denominator is not malformed and lands with no
/// ratio at all.
///
/// Perturbations: drop the turn check and the mid-turn verdict lands;
/// let a lone term through as no ratio and the half-ratio verdicts land.
/// Watched under each.
///
/// conforms: trace-score-records-the-verdict-and-its-terms
#[test]
fn a_score_out_of_place_or_malformed_is_refused() {
    let (answered, scores) = with_seat(Some(weaver_types::TurnKey("t-3".into())), |ports| {
        ports.score("reached-the-goal", true, None, None)
    });
    assert!(
        !answered && scores.is_empty(),
        "no score inside a turn: {scores:?}"
    );
    for (measured, denominator, what) in [
        (Some(14), None, "a measured count with no denominator"),
        (None, Some(11), "a denominator with no measured count"),
        (Some(14), Some(0), "a zero denominator"),
    ] {
        let (answered, scores) = with_seat(None, |ports| {
            ports.score("reached-the-goal", true, measured, denominator)
        });
        assert!(!answered && scores.is_empty(), "{what} refuses: {scores:?}");
    }
    let (answered, scores) = with_seat(None, |ports| ports.score("", true, None, None));
    assert!(!answered && scores.is_empty(), "an empty predicate refuses");
    let (answered, scores) = with_seat(None, |ports| {
        ports.score("reached-the-goal", false, None, None)
    });
    assert!(answered, "a verdict with no denominator lands");
    assert!(
        scores[0].contains(r#""payload":{"predicate":"reached-the-goal","passed":false}"#),
        "with no ratio at all: {}",
        scores[0]
    );
}

fn loaded_recorder() -> (crate::record::Record, Author) {
    let session = SessionId("s-1".into());
    let mut recorder = crate::record::Record::from(
        Recorder::receive(
            tempfile(),
            RunRef("r-1".into()),
            SessionRef(session.0.clone()),
        )
        .expect("recorder"),
    );
    let author = Author::new(&session, &weaver_types::RunId("r-1".into()));
    author
        .author(
            &mut recorder,
            Kind::Load,
            Subsystem::Harness,
            None,
            Some(Payload::Elections(Box::new(weaver_trace::Elections {
                residual_readout: false,
                field: None,
                surprisal: false,
                tee: Some(weaver_trace::Election::default()),
                state_member: true,
                declaration: Default::default(),
                lineage: None,
                reset: None,
                stack: Default::default(),
                boundary: String::new(),
                cause: weaver_trace::Cause { uid: 0 },
                library_path: None,
                state_store: Default::default(),
                composer: weaver_trace::LoopIdentity::compiled("test"),
            }))),
        )
        .expect("load");
    (recorder, author)
}

/// **The recall after a flush reaches the record between the flush and
/// the re-entry, and before the loop holds the answer**, per
/// `weaver-trace-Spec` section 3's recall clause, on the M1 record's
/// shape: `m1-002` flushed at sequence 255 and re-entered at 257, and
/// the `recall(4)` between them left no event, so the post-flush input
/// was drawn from an ask the record did not hold (#673, item 2).
///
/// The flush answers the counts that record carried, the loop asks for
/// the last four turns as the M1 loop does, and custody answers one
/// turnless prefix event and one turned one. The recall lands at the
/// flush's sequence plus one, turnless, naming the ask and each returned
/// event by run, turn, sequence and kind, and carrying none of their
/// pairs.
///
/// Perturbation: drop the `author_recall` call from `Ports::recall` and
/// the recall-count assertion fails, the answer still reaching the loop.
/// Watched under exactly that removal.
///
/// conforms: trace-recall-records-the-ask-and-its-identities
#[test]
fn the_recall_after_a_flush_is_recorded_before_the_loop_holds_it() {
    use std::io::{Read, Write};

    let (near, far) = socketpair(
        AddressFamily::Unix,
        SockType::SeqPacket,
        None,
        SockFlag::SOCK_CLOEXEC,
    )
    .expect("socketpair");
    let decode = crate::channel::decode_from_owned(near);
    let peer = std::thread::spawn(move || {
        let mut buf = vec![0u8; 65536];
        let n = recv(far.as_raw_fd(), &mut buf, MsgFlags::empty()).expect("recv flush");
        let directive: weaver_types::TokenDirective =
            serde_json::from_slice(&buf[..n]).expect("the flush parses");
        assert_eq!(directive, weaver_types::TokenDirective::Flush { keep: 0 });
        let answer = weaver_types::TokenAnswer::Flushed {
            resident_before: 27196,
            resident_after: 712,
        };
        let bytes = serde_json::to_vec(&answer).expect("answer renders");
        send(far.as_raw_fd(), &bytes, MsgFlags::empty()).expect("send answer");
    });

    let (ours, mut member) = std::os::unix::net::UnixStream::pair().expect("pair");
    ours.set_nonblocking(true).expect("nonblocking");
    let mut seam = crate::state::StateSeam::new(ours);
    member
        .write_all(
            concat!(
                r#"{"answer":{"recall":{"events":["#,
                r#"{"envelope":{"session":"s-1","run":"r-0","kind":"message.system","#,
                r#""sequence":"3"},"pairs":{"role":"system","content":"You are Karl."}},"#,
                r#"{"envelope":{"session":"s-1","run":"r-1","turn":"t-36","#,
                r#""kind":"message.assistant","sequence":"250"},"#,
                r#""pairs":{"role":"assistant","content":"the plan"}}]}}}"#,
                "\n"
            )
            .as_bytes(),
        )
        .expect("custody answers in advance");

    let (mut recorder, author) = loaded_recorder();
    let mut turn_ordinal = 36u64;
    let mut turn_in_flight: Option<weaver_types::TurnKey> = None;
    let (listener, _dir) = test_listener();
    let mut fullness = Some((27196, 32768));
    let mut pressure_reported = false;
    let load_facts = crate::engine::test_load_facts();
    let answered = {
        let mut ports = Ports::grant(
            &decode,
            &author,
            &mut recorder,
            &mut turn_ordinal,
            &mut turn_in_flight,
            &load_facts,
            None,
            &listener,
            None,
            None,
            Some(&mut seam),
            None,
            &mut fullness,
            &mut pressure_reported,
        );
        assert_eq!(ports.flush(0), Some((27196, 712)), "the flush holds");
        let answered = ports.recall(Some(4)).expect("custody answered");
        // The loop holds the answer from here, and the record already
        // carries it: read the structure while the grant still stands.
        let recalls = ports
            .recorder
            .structure()
            .expect("the serving record")
            .iter()
            .filter(|r| r.kind == Kind::Recall)
            .count();
        assert_eq!(
            recalls, 1,
            "the recall is recorded before the loop holds it"
        );
        answered
    };
    peer.join().expect("the decode peer finishes");
    assert_eq!(answered.len(), 2, "the loop receives what custody answered");

    let mut asked = [0u8; 128];
    let n = member.read(&mut asked).expect("reads the ask");
    assert_eq!(&asked[..n], b"{\"ask\":{\"recall\":{\"last-turns\":4}}}\n");

    let records = recorder.structure().expect("the serving record");
    let flush = records
        .iter()
        .find(|r| r.kind == Kind::Flush)
        .expect("the flush is recorded");
    let recall = records
        .iter()
        .find(|r| r.kind == Kind::Recall)
        .expect("the recall is recorded");
    let flush: serde_json::Value = serde_json::from_str(flush.line.as_ref()).expect("parses");
    let line: &str = recall.line.as_ref();
    let recall: serde_json::Value = serde_json::from_str(line).expect("parses");
    let sequence = |event: &serde_json::Value| -> u64 {
        event["sequence"]
            .as_str()
            .expect("a decimal string")
            .parse()
            .expect("a number")
    };
    assert_eq!(
        sequence(&recall),
        sequence(&flush) + 1,
        "the recall follows the flush with nothing between, where the re-entry comes next"
    );
    assert!(recall.get("turn").is_none(), "a recall belongs to no turn");
    assert_eq!(recall["subsystem"], "harness");
    assert!(
        line.contains(concat!(
            r#""payload":{"ask":{"verb":"recall","last_turns":4},"returned":["#,
            r#"{"run":"r-0","sequence":"3","kind":"message.system"},"#,
            r#"{"run":"r-1","turn":"t-36","sequence":"250","kind":"message.assistant"}]}"#
        )),
        "the ask and the identities, in declared order: {line}"
    );
    assert!(
        !line.contains("You are Karl.") && !line.contains("the plan"),
        "and none of the returned events' contents: {line}"
    );
}

/// **A whole-session replay answer is recorded by its bounds and its
/// count, never as a list as long as the session**, per
/// `weaver-trace-Spec` section 3's recall clause, and before the loop
/// walks it. Three events answer, so the middle one is what a list would
/// have carried and the bounds do not.
///
/// Perturbation: record every identity for the replay verb and the
/// `returned` assertion fails, three entries where two stand; drop the
/// `author_recall` call from `Ports::replay` and the count assertion
/// fails. Watched under both.
///
/// conforms: trace-recall-records-the-ask-and-its-identities
#[test]
fn a_replay_answer_is_recorded_by_its_bounds_and_count() {
    use std::io::Write;

    let (near, _far) = socketpair(
        AddressFamily::Unix,
        SockType::SeqPacket,
        None,
        SockFlag::SOCK_CLOEXEC,
    )
    .expect("socketpair");
    let decode = crate::channel::decode_from_owned(near);
    let (ours, mut member) = std::os::unix::net::UnixStream::pair().expect("pair");
    ours.set_nonblocking(true).expect("nonblocking");
    let mut seam = crate::state::StateSeam::new(ours);
    member
        .write_all(
            concat!(
                r#"{"answer":{"replay":{"events":["#,
                r#"{"envelope":{"kind":"message.system","run":"r-0","sequence":"1"},"pairs":{}},"#,
                r#"{"envelope":{"kind":"model.request","run":"r-0","turn":"t-1","sequence":"4"},"pairs":{}},"#,
                r#"{"envelope":{"kind":"turn.closed","run":"r-0","turn":"t-1","sequence":"9"},"pairs":{}}"#,
                r#"]}}}"#,
                "\n"
            )
            .as_bytes(),
        )
        .expect("custody answers in advance");

    let (mut recorder, author) = loaded_recorder();
    let mut turn_ordinal = 0u64;
    let mut turn_in_flight: Option<weaver_types::TurnKey> = None;
    let (listener, _dir) = test_listener();
    let mut fullness = None;
    let mut pressure_reported = false;
    let load_facts = crate::engine::test_load_facts();
    let answered = {
        let mut ports = Ports::grant(
            &decode,
            &author,
            &mut recorder,
            &mut turn_ordinal,
            &mut turn_in_flight,
            &load_facts,
            None,
            &listener,
            None,
            None,
            Some(&mut seam),
            None,
            &mut fullness,
            &mut pressure_reported,
        );
        ports.replay(2_000).expect("custody answered")
    };
    assert_eq!(answered.len(), 3, "the loop walks the whole answer");

    let records = recorder.structure().expect("the serving record");
    let recalls: Vec<serde_json::Value> = records
        .iter()
        .filter(|r| r.kind == Kind::Recall)
        .map(|r| serde_json::from_str(r.line.as_ref()).expect("parses"))
        .collect();
    assert_eq!(recalls.len(), 1, "the replay ask is recorded once");
    let payload = &recalls[0]["payload"];
    assert_eq!(payload["ask"], serde_json::json!({"verb": "replay"}));
    assert_eq!(payload["count"], 3, "the events the member answered");
    assert_eq!(
        payload["returned"],
        serde_json::json!([
            {"run": "r-0", "sequence": "1", "kind": "message.system"},
            {"run": "r-0", "turn": "t-1", "sequence": "9", "kind": "turn.closed"}
        ]),
        "named by its first and last events and nothing between"
    );
}

/// **A recall custody did not answer is not a recall event**, per
/// `weaver-trace-Spec` section 3's recall clause: a dead seam answers
/// nothing, and the loop reads `None` with the record unchanged.
///
/// Perturbation: author the recall before the ask answers, with an
/// empty identity list for a miss, and the count assertion fails.
#[test]
fn a_recall_the_member_did_not_answer_records_no_recall() {
    let (near, _far) = socketpair(
        AddressFamily::Unix,
        SockType::SeqPacket,
        None,
        SockFlag::SOCK_CLOEXEC,
    )
    .expect("socketpair");
    let decode = crate::channel::decode_from_owned(near);
    let (ours, member) = std::os::unix::net::UnixStream::pair().expect("pair");
    ours.set_nonblocking(true).expect("nonblocking");
    drop(member);
    let mut seam = crate::state::StateSeam::new(ours);

    let (mut recorder, author) = loaded_recorder();
    let mut turn_ordinal = 0u64;
    let mut turn_in_flight: Option<weaver_types::TurnKey> = None;
    let (listener, _dir) = test_listener();
    let mut fullness = None;
    let mut pressure_reported = false;
    let load_facts = crate::engine::test_load_facts();
    {
        let mut ports = Ports::grant(
            &decode,
            &author,
            &mut recorder,
            &mut turn_ordinal,
            &mut turn_in_flight,
            &load_facts,
            None,
            &listener,
            None,
            None,
            Some(&mut seam),
            None,
            &mut fullness,
            &mut pressure_reported,
        );
        assert!(
            ports.recall(Some(4)).is_none(),
            "a dead member answers nothing"
        );
    }
    assert_eq!(
        recorder
            .structure()
            .expect("the serving record")
            .iter()
            .filter(|r| r.kind == Kind::Recall)
            .count(),
        0,
        "and the record holds no recall for an ask nothing answered"
    );
}

fn test_listener() -> (
    crate::channel::CoordinationListener,
    crate::scratch::Scratch,
) {
    let dir = crate::scratch::dir(format!(
        "weaver-engine-{}-{:?}",
        std::process::id(),
        std::thread::current().id()
    ));
    let listener = crate::channel::bind_coordination(&dir.join("c.sock")).expect("bind");
    (listener, dir)
}

fn tempfile() -> OwnedFd {
    let path = std::env::temp_dir().join(format!(
        "weaver-harness-turn-{}-{:?}",
        std::process::id(),
        std::thread::current().id()
    ));
    let file = std::fs::File::create(&path).expect("sink");
    std::fs::remove_file(&path).ok();
    OwnedFd::from(file)
}

/// **An unrecovered call is authored at the answer and handed to the loop**,
/// per `weaver-harness-Spec` sections 6.1 and 8 on the operator's ruling of
/// 2026-10-13 (#31): one `model.unrecovered_call` per report entry, in order,
/// after `model.measurement` and before `message.assistant`, each fragment
/// the same bytes as its block, and the report carried on the outcome.
///
/// Perturbations: remove the authoring and the kinds fail; move it after
/// `author_message` and the order fails; drop the report from the outcome
/// and the length fails.
#[test]
fn an_unrecovered_call_is_authored_at_the_answer_and_handed_to_the_loop() {
    let (near, far) = socketpair(
        AddressFamily::Unix,
        SockType::SeqPacket,
        None,
        SockFlag::SOCK_CLOEXEC,
    )
    .expect("socketpair");
    let decode = crate::channel::decode_from_owned(near);

    // The scripted decode peer: read the append, stream two tokens, then
    // answer with the generation whole, the request and measurement the
    // SPU-rendered blobs the turn splices.
    let peer = std::thread::spawn(move || {
        let mut buf = vec![0u8; 65536];
        let n = recv(far.as_raw_fd(), &mut buf, MsgFlags::empty()).expect("recv append");
        let directive: weaver_types::TokenDirective =
            serde_json::from_slice(&buf[..n]).expect("the append parses");
        assert!(
            matches!(
                directive,
                weaver_types::TokenDirective::AppendAndGenerate { .. }
            ),
            "the turn sends append-and-generate"
        );
        let send_answer = |answer: &weaver_types::TokenAnswer| {
            let bytes = serde_json::to_vec(answer).expect("answer renders");
            send(far.as_raw_fd(), &bytes, MsgFlags::empty()).expect("send answer");
        };
        send_answer(&weaver_types::TokenAnswer::Token {
            token: 9707,
            piece: "one".into(),
        });
        send_answer(&weaver_types::TokenAnswer::Token {
            token: 1879,
            piece: " word".into(),
        });
        let request = serde_json::value::RawValue::from_string(
            r#"{"rendered":"<|im_start|>user\nsay a word<|im_end|>\n","template":"qwen2","sampling":{"temperature":0.7}}"#
                .to_string(),
        )
        .unwrap();
        let measurement = serde_json::value::RawValue::from_string(
            r#"{"model":"qwen2.5","weights_hash":"sha256:abc","input_tokens":[1,2],"output_tokens":[9707,1879],"blocks":[{"label":"turn-delta","start":0,"end":10}],"timings":{"prefill_ns":"10","decode_ns":"20"}}"#
                .to_string(),
        )
        .unwrap();
        send_answer(&weaver_types::TokenAnswer::Generated(Generation {
            content: vec![
                weaver_traits::ContentBlock::Text {
                    text: "one word".into(),
                },
                weaver_traits::ContentBlock::UnrecoveredCall {
                    fragment: "<tool_call>x</tool_call>".into(),
                },
                weaver_traits::ContentBlock::UnrecoveredCall {
                    fragment: "<tool_call>y".into(),
                },
            ],
            unrecovered: vec![
                unrecovered_entry("<tool_call>x</tool_call>", false),
                unrecovered_entry("<tool_call>y", true),
            ],
            emission: "one word".into(),
            finish: Finish::Completed,
            // Deliberately unroundable: a site forwarding a zero, a
            // default, or the other member would fail the assertion
            // below, where 64 and 4096 could each be arrived at by
            // accident.
            resident: 1237,
            capacity: 8191,
            request,
            measurement,
        }));
    });

    let session = SessionId("s-1".into());
    let sink = tempfile();
    let mut recorder = crate::record::Record::from(
        Recorder::receive(sink, RunRef("r-1".into()), SessionRef(session.0.clone()))
            .expect("recorder"),
    );
    let author = Author::new(&session, &weaver_types::RunId("r-1".into()));
    author
        .author(
            &mut recorder,
            Kind::Load,
            Subsystem::Harness,
            None,
            Some(Payload::Elections(Box::new(weaver_trace::Elections {
                residual_readout: false,
                field: None,
                surprisal: false,
                tee: Some(weaver_trace::Election::default()),
                state_member: false,
                declaration: Default::default(),
                lineage: None,
                reset: None,
                stack: Default::default(),
                boundary: String::new(),
                cause: weaver_trace::Cause { uid: 0 },
                library_path: None,
                state_store: Default::default(),
                composer: weaver_trace::LoopIdentity::compiled("test"),
            }))),
        )
        .expect("load");
    let mut turn_ordinal = 0u64;
    let mut turn_in_flight: Option<weaver_types::TurnKey> = None;

    let (listener, _dir) = test_listener();
    let outcome = {
        let mut fullness = None;
        let mut pressure_reported = false;
        let load_facts = crate::engine::test_load_facts();
        let mut ports = Ports::grant(
            &decode,
            &author,
            &mut recorder,
            &mut turn_ordinal,
            &mut turn_in_flight,
            &load_facts,
            None,
            &listener,
            None,
            None,
            None,
            None,
            &mut fullness,
            &mut pressure_reported,
        );
        let delta = vec![Message {
            role: Role::User,
            content: vec![ContentBlock::Text {
                text: "say a word".into(),
            }],
        }];
        ports.turn(delta).expect("the turn completes")
    };
    peer.join().expect("the decode peer finishes");

    assert_eq!(outcome.emission, "one word", "loop 1 receives the emission");
    assert!(!outcome.stopped, "a clean generation is not stopped");

    // The bracket, in order: the turn's user message, the three model
    // events, the assistant's turn, and the close.
    let kinds: Vec<Kind> = recorder
        .structure()
        .expect("the serving record")
        .iter()
        .filter(|r| r.turn.is_some())
        .map(|r| r.kind)
        .collect();
    assert_eq!(
        kinds,
        vec![
            Kind::TurnStarted,
            Kind::MessageUser,
            Kind::ModelRequest,
            Kind::ModelOutput,
            Kind::ModelMeasurement,
            Kind::ModelUnrecoveredCall,
            Kind::ModelUnrecoveredCall,
            Kind::MessageAssistant,
            Kind::TurnClosed,
        ],
        "one event per entry, after the measurement and before the assistant"
    );

    // **Each event's fragment is its block's**, in the same order.
    let fragments: Vec<String> = recorder
        .structure()
        .expect("the serving record")
        .by_kind(Kind::ModelUnrecoveredCall)
        .map(|r| {
            let line: serde_json::Value = serde_json::from_str(&r.line).expect("one value");
            assert_eq!(line["subsystem"], "spu_decoder", "{line}");
            line["payload"]["fragment"]
                .as_str()
                .expect("a fragment")
                .to_string()
        })
        .collect();
    assert_eq!(
        fragments,
        vec![
            "<tool_call>x</tool_call>".to_string(),
            "<tool_call>y".to_string()
        ]
    );
    let assistant: serde_json::Value = serde_json::from_str(
        &recorder
            .structure()
            .expect("the serving record")
            .by_kind(Kind::MessageAssistant)
            .next()
            .expect("the assistant authored")
            .line,
    )
    .expect("one value");
    let blocks: Vec<String> = assistant["payload"]["content"]
        .as_array()
        .expect("the content")
        .iter()
        .filter(|block| block["type"] == "unrecovered_call")
        .map(|block| block["fragment"].as_str().expect("a fragment").to_string())
        .collect();
    assert_eq!(
        blocks, fragments,
        "each event's fragment is its block's, in order: {assistant}"
    );

    // **Loop 1 receives the report with the outcome.**
    assert_eq!(outcome.unrecovered.len(), 2, "the report reaches loop 1");
    assert_eq!(
        outcome.unrecovered[1].reason,
        weaver_types::UnrecoveredReason::Unclosed
    );
}

/// **An `UnrecoveredCall` block opens no execution exchange**, per
/// `weaver-harness-Spec` sections 6 and 8: an emission holding one recovered
/// call and one unrecovered opens exactly one exchange, the gate peer
/// answering one and the record carrying one `tool.call.started`.
///
/// Perturbation: let the dispatch take the unrecovered block as a call too and
/// a second exchange opens against a peer that answers one, so the turn fails.
#[test]
fn an_unrecovered_call_opens_no_execution_exchange() {
    let (near, far) = socketpair(
        AddressFamily::Unix,
        SockType::SeqPacket,
        None,
        SockFlag::SOCK_CLOEXEC,
    )
    .expect("socketpair");
    let decode = crate::channel::decode_from_owned(near);

    // The decode peer: two rounds. Round one answers a generation whose
    // canonical content carries the call; round two reads the tool-result
    // delta back and answers plain text.
    let peer = std::thread::spawn(move || {
        let mut buf = vec![0u8; 65536];
        let raw = |text: &str| serde_json::value::RawValue::from_string(text.to_string()).unwrap();
        let send_answer = |answer: &weaver_types::TokenAnswer| {
            let bytes = serde_json::to_vec(answer).expect("answer renders");
            send(far.as_raw_fd(), &bytes, MsgFlags::empty()).expect("send answer");
        };

        let n = recv(far.as_raw_fd(), &mut buf, MsgFlags::empty()).expect("recv append");
        let directive: weaver_types::TokenDirective =
            serde_json::from_slice(&buf[..n]).expect("the append parses");
        assert!(matches!(
            directive,
            weaver_types::TokenDirective::AppendAndGenerate { .. }
        ));
        send_answer(&weaver_types::TokenAnswer::Generated(Generation {
            content: vec![
                weaver_traits::ContentBlock::ToolCall(weaver_traits::ToolCall {
                    name: "sh".into(),
                    arguments: r#"{"expression":"37 * 43"}"#.into(),
                }),
                weaver_traits::ContentBlock::UnrecoveredCall {
                    fragment: "<tool_call>z".into(),
                },
            ],
            unrecovered: vec![unrecovered_entry("<tool_call>z", true)],
            emission: r#"<tool_call>{"name":"sh"}</tool_call>"#.into(),
            finish: Finish::Completed,
            resident: 64,
            capacity: 4096,
            request: raw(r#"{"rendered":"r1","template":"qwen2","sampling":{}}"#),
            measurement: raw(
                r#"{"model":"m","weights_hash":"h","input_tokens":[1],"output_tokens":[2],"blocks":[{"label":"turn-delta","start":0,"end":1}],"timings":{"prefill_ns":"1","decode_ns":"2"}}"#,
            ),
        }));

        // Round two: the tool-result delta arrives, carrying the granted
        // content and nothing the loop invented.
        let n = recv(far.as_raw_fd(), &mut buf, MsgFlags::empty()).expect("recv round 2");
        let directive: weaver_types::TokenDirective =
            serde_json::from_slice(&buf[..n]).expect("round 2 parses");
        let weaver_types::TokenDirective::AppendAndGenerate { delta, .. } = directive else {
            panic!("round two is an append");
        };
        assert_eq!(delta.len(), 1, "one tool-result message crosses");
        assert!(matches!(delta[0].role, Role::ToolResult));
        assert!(
            matches!(
                &delta[0].content[0],
                ContentBlock::ToolResult(block) if block.content == "1591"
            ),
            "the granted content is what crosses: {:?}",
            delta[0].content
        );
        send_answer(&weaver_types::TokenAnswer::Generated(Generation {
            content: vec![weaver_traits::ContentBlock::Text {
                text: "37 * 43 is 1591.".into(),
            }],
            unrecovered: vec![],
            emission: "37 * 43 is 1591.".into(),
            finish: Finish::Completed,
            resident: 64,
            capacity: 4096,
            request: raw(r#"{"rendered":"r2","template":"qwen2","sampling":{}}"#),
            measurement: raw(
                r#"{"model":"m","weights_hash":"h","input_tokens":[3],"output_tokens":[4],"blocks":[{"label":"turn-delta","start":0,"end":1}],"timings":{"prefill_ns":"1","decode_ns":"2"}}"#,
            ),
        }));
    });

    // The gate peer: one execution exchange, answered with the result,
    // exactly as the gate's dispatch would.
    let (gate_near, gate_child) = crate::channel::OrganChannel::pair().expect("gate pair");
    let gate_peer = std::thread::spawn(move || {
        let gate_far = gate_child.into_channel();
        let envelope = gate_far.recv().expect("the execution opens");
        assert_eq!(envelope.position, weaver_types::Position::Open);
        let weaver_types::Payload::Tool(execution) = &envelope.payload else {
            panic!("the exchange carries the call, got {:?}", envelope.payload);
        };
        assert_eq!(execution.name.0, "sh");
        let outcome = weaver_gate_execute_stub(execution);
        gate_far
            .send(&weaver_types::OrganEnvelope {
                exchange: envelope.exchange.clone(),
                position: weaver_types::Position::Close,
                payload: weaver_types::Payload::ToolAnswer(outcome),
            })
            .expect("the answer closes");
    });

    let session = SessionId("s-t".into());
    let sink = tempfile();
    let mut recorder = crate::record::Record::from(
        Recorder::receive(sink, RunRef("r-1".into()), SessionRef(session.0.clone()))
            .expect("recorder"),
    );
    let author = Author::new(&session, &weaver_types::RunId("r-1".into()));
    author
        .author(
            &mut recorder,
            Kind::Load,
            Subsystem::Harness,
            None,
            Some(Payload::Elections(Box::new(weaver_trace::Elections {
                residual_readout: false,
                field: None,
                surprisal: false,
                tee: Some(weaver_trace::Election::default()),
                state_member: false,
                declaration: Default::default(),
                lineage: None,
                reset: None,
                stack: Default::default(),
                boundary: String::new(),
                cause: weaver_trace::Cause { uid: 0 },
                library_path: None,
                state_store: Default::default(),
                composer: weaver_trace::LoopIdentity::compiled("test"),
            }))),
        )
        .expect("load");
    let mut turn_ordinal = 0u64;
    let mut turn_in_flight: Option<weaver_types::TurnKey> = None;
    let mut gate_ordinal = 0u64;
    let mut held = std::collections::VecDeque::new();

    let (listener, _dir) = test_listener();
    let outcome = {
        let mut fullness = None;
        let mut pressure_reported = false;
        let load_facts = crate::engine::test_load_facts();
        let mut ports = Ports::grant(
            &decode,
            &author,
            &mut recorder,
            &mut turn_ordinal,
            &mut turn_in_flight,
            &load_facts,
            None,
            &listener,
            None,
            Some(GatePort {
                channel: &gate_near,
                ordinal: &mut gate_ordinal,
                held: &mut held,
                tools: None,
                contained: None,
                shell_granted: true,
            }),
            None,
            None,
            &mut fullness,
            &mut pressure_reported,
        );
        let delta = vec![Message {
            role: Role::User,
            content: vec![ContentBlock::Text {
                text: "what is 37 * 43?".into(),
            }],
        }];
        ports.turn(delta).expect("the turn completes")
    };
    peer.join().expect("the decode peer finishes");
    gate_peer.join().expect("the gate peer finishes");

    assert_eq!(outcome.emission, "37 * 43 is 1591.");
    assert!(held.is_empty(), "nothing crossed mid-execution to hold");

    let kinds: Vec<Kind> = recorder
        .structure()
        .expect("the serving record")
        .iter()
        .filter(|r| r.turn.is_some())
        .map(|r| r.kind)
        .collect();
    assert_eq!(
        kinds,
        vec![
            Kind::TurnStarted,
            Kind::MessageUser,
            Kind::ModelRequest,
            Kind::ModelOutput,
            Kind::ModelMeasurement,
            Kind::ModelUnrecoveredCall,
            Kind::MessageAssistant,
            Kind::ToolCallStarted,
            Kind::ToolCallCompleted,
            Kind::MessageToolResult,
            Kind::ModelRequest,
            Kind::ModelOutput,
            Kind::ModelMeasurement,
            Kind::MessageAssistant,
            Kind::TurnClosed,
        ],
        "one exchange for the recovered call and none for the unrecovered one"
    );

    // The attribution: started is the harness's dispatch, completed is
    // the gate's answer.
    let line_of = |kind: Kind| {
        recorder
            .structure()
            .expect("the serving record")
            .by_kind(kind)
            .next()
            .expect("authored")
            .line
            .to_string()
    };
    assert!(line_of(Kind::ToolCallStarted).contains("\"subsystem\":\"harness\""));
    assert!(line_of(Kind::ToolCallCompleted).contains("\"subsystem\":\"gate\""));
    assert!(
        line_of(Kind::MessageToolResult).contains("1591"),
        "the granted content is the record's"
    );
}

/// One report entry for a scripted generation, the grammar qwen2's.
fn unrecovered_entry(fragment: &str, unclosed: bool) -> weaver_types::UnrecoveredCall {
    weaver_types::UnrecoveredCall {
        fragment: fragment.to_string(),
        reason: if unclosed {
            weaver_types::UnrecoveredReason::Unclosed
        } else {
            weaver_types::UnrecoveredReason::Unparsed {
                detail: "not JSON".to_string(),
            }
        },
        grammar: weaver_types::CallGrammar {
            family: "qwen2".to_string(),
            open: "<tool_call>".to_string(),
            close: Some("</tool_call>".to_string()),
        },
    }
}

/// **A re-fed answer's unrecovered call is authored at the answer**, per
/// `weaver-diagnostic-Spec` section 7 on the operator's ruling of 2026-10-13
/// (#31): the re-feed's authoring site calls the same helper the generation's
/// does, and a record under a diagnostic binding converts the kind, which
/// `record::tests::an_unrecovered_call_reaches_the_diagnostic_record` watches.
///
/// Perturbation: remove the helper call from `refeed` and the kinds fail.
#[test]
fn a_re_fed_unrecovered_call_is_authored_at_the_answer() {
    let (near, far) = socketpair(
        AddressFamily::Unix,
        SockType::SeqPacket,
        None,
        SockFlag::SOCK_CLOEXEC,
    )
    .expect("socketpair");
    let decode = crate::channel::decode_from_owned(near);

    // The scripted decode peer: read the append, stream two tokens, then
    // answer with the generation whole, the request and measurement the
    // SPU-rendered blobs the turn splices.
    let peer = std::thread::spawn(move || {
        let mut buf = vec![0u8; 65536];
        let n = recv(far.as_raw_fd(), &mut buf, MsgFlags::empty()).expect("recv append");
        let directive: weaver_types::TokenDirective =
            serde_json::from_slice(&buf[..n]).expect("the append parses");
        assert!(
            matches!(directive, weaver_types::TokenDirective::ReFeed { .. }),
            "the replay sends the re-feed"
        );
        let send_answer = |answer: &weaver_types::TokenAnswer| {
            let bytes = serde_json::to_vec(answer).expect("answer renders");
            send(far.as_raw_fd(), &bytes, MsgFlags::empty()).expect("send answer");
        };
        // A re-feed streams no token of its own, per the decode contract.
        let request = serde_json::value::RawValue::from_string(
            r#"{"rendered":"<|im_start|>user\nsay a word<|im_end|>\n","template":"qwen2","sampling":{"temperature":0.7}}"#
                .to_string(),
        )
        .unwrap();
        let measurement = serde_json::value::RawValue::from_string(
            r#"{"model":"qwen2.5","weights_hash":"sha256:abc","input_tokens":[1,2],"output_tokens":[9707,1879],"blocks":[{"label":"turn-delta","start":0,"end":10}],"timings":{"prefill_ns":"10","decode_ns":"20"}}"#
                .to_string(),
        )
        .unwrap();
        send_answer(&weaver_types::TokenAnswer::ReFed(Generation {
            content: vec![
                weaver_traits::ContentBlock::Text {
                    text: "one word".into(),
                },
                weaver_traits::ContentBlock::UnrecoveredCall {
                    fragment: "<tool_call>x</tool_call>".into(),
                },
                weaver_traits::ContentBlock::UnrecoveredCall {
                    fragment: "<tool_call>y".into(),
                },
            ],
            unrecovered: vec![
                unrecovered_entry("<tool_call>x</tool_call>", false),
                unrecovered_entry("<tool_call>y", true),
            ],
            emission: "one word".into(),
            finish: Finish::Completed,
            // Deliberately unroundable: a site forwarding a zero, a
            // default, or the other member would fail the assertion
            // below, where 64 and 4096 could each be arrived at by
            // accident.
            resident: 1237,
            capacity: 8191,
            request,
            measurement,
        }));
    });

    let session = SessionId("s-1".into());
    let sink = tempfile();
    let mut recorder = crate::record::Record::from(
        Recorder::receive(sink, RunRef("r-1".into()), SessionRef(session.0.clone()))
            .expect("recorder"),
    );
    let author = Author::new(&session, &weaver_types::RunId("r-1".into()));
    author
        .author(
            &mut recorder,
            Kind::Load,
            Subsystem::Harness,
            None,
            Some(Payload::Elections(Box::new(weaver_trace::Elections {
                residual_readout: false,
                field: None,
                surprisal: false,
                tee: Some(weaver_trace::Election::default()),
                state_member: false,
                declaration: Default::default(),
                lineage: None,
                reset: None,
                stack: Default::default(),
                boundary: String::new(),
                cause: weaver_trace::Cause { uid: 0 },
                library_path: None,
                state_store: Default::default(),
                composer: weaver_trace::LoopIdentity::compiled("test"),
            }))),
        )
        .expect("load");
    let mut turn_ordinal = 0u64;
    let mut turn_in_flight: Option<weaver_types::TurnKey> = None;

    let (listener, _dir) = test_listener();
    let outcome = {
        let mut fullness = None;
        let mut pressure_reported = false;
        let load_facts = crate::engine::test_load_facts();
        let mut ports = Ports::grant(
            &decode,
            &author,
            &mut recorder,
            &mut turn_ordinal,
            &mut turn_in_flight,
            &load_facts,
            None,
            &listener,
            None,
            None,
            None,
            None,
            &mut fullness,
            &mut pressure_reported,
        );
        let turn = weaver_types::TurnKey("t-1".into());
        ports
            .refeed(&turn, "rendered".into(), vec![9707, 1879])
            .expect("the re-feed answers")
    };
    peer.join().expect("the decode peer finishes");

    assert_eq!(
        outcome.unrecovered.len(),
        2,
        "the answer carried both entries"
    );

    // **The re-feed's authoring is the generation's**, per
    // `weaver-diagnostic-Spec` section 7: the report lands between the
    // measurement and the assistant, one event per entry.
    let kinds: Vec<Kind> = recorder
        .structure()
        .expect("the serving record")
        .iter()
        .filter(|r| r.turn.is_some())
        .map(|r| r.kind)
        .collect();
    assert_eq!(
        kinds,
        vec![
            Kind::ModelRequest,
            Kind::ModelOutput,
            Kind::ModelMeasurement,
            Kind::ModelUnrecoveredCall,
            Kind::ModelUnrecoveredCall,
            Kind::MessageAssistant,
        ],
        "the re-feed authors the report at its answer"
    );
}
