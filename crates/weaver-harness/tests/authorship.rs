//! conforms: harness-licensed-combinations-refused-before-submit
//! conforms: harness-timestamps-stamped-at-authoring
//! conforms: harness-assembly-kind-filter-at-read-site
//! conforms: harness-deterministic-assembly
//! conforms: harness-prompt-part-order
//!
//! The authorship and assembly tests of `weaver-harness-Spec` section 8,
//! including the fourth walk. Each names its perturbation.

use std::fs::File;
use std::os::fd::OwnedFd;

use weaver_harness::{Author, Record, assemble, licensed};
use weaver_trace::{Kind, Payload, Recorder, RunRef, SessionRef, Subsystem, raw_payload};
use weaver_traits::{ContentBlock, Message, Role, ToolCall, ToolResultBlock};
use weaver_types::{SessionId, TurnKey};

fn recorder() -> Record {
    let path = std::env::temp_dir().join(format!(
        "weaver-harness-author-{}-{:?}",
        std::process::id(),
        std::thread::current().id()
    ));
    let file = File::create(&path).expect("sink");
    // Nothing reads the sink by name, so the name goes at once and the
    // recorder writes through its descriptor: no run leaves the file behind,
    // pass or fail (#690 item C2.9).
    let _ = std::fs::remove_file(&path);
    Record::Serving(
        Recorder::receive(
            OwnedFd::from(file),
            RunRef("r-1".into()),
            SessionRef("s-1".into()),
        )
        .expect("receives"),
    )
}

fn author() -> (Author, Record) {
    let session = SessionId("s-1".to_string());
    (
        Author::new(&session, &weaver_types::RunId("r-1".into())),
        recorder(),
    )
}

/// **The licensed combinations are enforced here, before submit.** The
/// recorder cannot hold this rule - it judges the envelope and never the
/// interior - so the harness is the party the rule binds.
///
/// Perturbation: remove the `licensed` call from `author_message` and the
/// unlicensed message reaches the recorder, which admits it. Watched under
/// exactly that removal.
#[test]
fn unlicensed_message_is_refused_before_submit() {
    let (author, mut recorder) = author();
    author
        .author(
            &mut recorder,
            Kind::Load,
            Subsystem::Harness,
            None,
            Some(Payload::Elections(weaver_trace::Elections {
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
            })),
        )
        .expect("load");
    let turn = TurnKey("t-1".to_string());
    author
        .author(
            &mut recorder,
            Kind::TurnStarted,
            Subsystem::Harness,
            Some(&turn),
            None,
        )
        .expect("turn started");

    let unlicensed = Message {
        role: Role::Assistant,
        content: vec![ContentBlock::ToolResult(ToolResultBlock {
            content: "42".into(),
        })],
    };
    let before = recorder.structure().expect("the serving record").len();
    let refused = author.author_message(&mut recorder, &unlicensed, &turn);
    assert!(
        refused.is_err(),
        "an assistant message carrying a tool result is unlicensed"
    );
    assert_eq!(
        recorder.structure().expect("the serving record").len(),
        before,
        "it never reached the recorder"
    );

    let licensed_message = Message {
        role: Role::Assistant,
        content: vec![
            ContentBlock::Text {
                text: "calling the calculator".into(),
            },
            ContentBlock::ToolCall(ToolCall {
                name: "calculator".into(),
                arguments: "{}".into(),
            }),
        ],
    };
    author
        .author_message(&mut recorder, &licensed_message, &turn)
        .expect("licensed")
        .expect("submitted");
    assert_eq!(
        recorder.structure().expect("the serving record").len(),
        before + 1
    );
}

/// Every licensed pairing is admitted and every unlicensed one refused, which
/// is the rule stated as a table rather than as one example.
#[test]
fn the_licensing_table_holds_in_both_directions() {
    let text = || ContentBlock::Text { text: "x".into() };
    let call = || {
        ContentBlock::ToolCall(ToolCall {
            name: "calculator".into(),
            arguments: "{}".into(),
        })
    };
    let result = || {
        ContentBlock::ToolResult(ToolResultBlock {
            content: "42".into(),
        })
    };

    assert!(
        licensed(&Message {
            role: Role::User,
            content: vec![text()]
        })
        .is_ok()
    );
    assert!(
        licensed(&Message {
            role: Role::Assistant,
            content: vec![text(), call()]
        })
        .is_ok()
    );
    assert!(
        licensed(&Message {
            role: Role::ToolResult,
            content: vec![result()]
        })
        .is_ok()
    );

    assert!(
        licensed(&Message {
            role: Role::User,
            content: vec![call()]
        })
        .is_err()
    );
    assert!(
        licensed(&Message {
            role: Role::User,
            content: vec![result()]
        })
        .is_err()
    );
    assert!(
        licensed(&Message {
            role: Role::Assistant,
            content: vec![result()]
        })
        .is_err()
    );
    assert!(
        licensed(&Message {
            role: Role::ToolResult,
            content: vec![text()]
        })
        .is_err()
    );
}

/// Both timestamps are stamped at authoring: the monotonic reading is
/// nanoseconds since the run's origin, an origin only the author holds, and it
/// advances across events within one run.
#[test]
fn timestamps_are_stamped_at_authoring() {
    let (author, mut recorder) = author();
    author
        .author(
            &mut recorder,
            Kind::Load,
            Subsystem::Harness,
            None,
            Some(Payload::Elections(weaver_trace::Elections {
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
            })),
        )
        .expect("load");
    author
        .author(
            &mut recorder,
            Kind::SessionClosed,
            Subsystem::Harness,
            None,
            None,
        )
        .expect("second");
    let lines: Vec<String> = recorder
        .structure()
        .expect("the serving record")
        .iter()
        .map(|r| r.line.to_string())
        .collect();
    let readings: Vec<u64> = lines
        .iter()
        .map(|l| {
            let v: serde_json::Value = serde_json::from_str(l).expect("line");
            v["monotonic_ns"]
                .as_str()
                .expect("decimal string")
                .parse()
                .expect("u64")
        })
        .collect();
    assert!(
        readings[1] >= readings[0],
        "the monotonic reading advances within the run"
    );
    let first: serde_json::Value = serde_json::from_str(&lines[0]).expect("line");
    assert!(first["wall_ms"].as_u64().expect("bare number") > 1_700_000_000_000);
}

/// **The fourth walk: the model reaches its own record through the prompt.**
/// The assembly path cannot see measurement, lifecycle, or custody events -
/// the filter is the kind set at the read site, not a judgment applied after a
/// full read.
///
/// Perturbation: widen the filter (or drop it and iterate every record) and
/// the measurement's content appears in the assembled prompt. Watched under
/// exactly that widening.
#[test]
fn assembly_sees_only_message_kinds() {
    let (author, mut recorder) = author();
    let turn = TurnKey("t-1".to_string());
    author
        .author(
            &mut recorder,
            Kind::Load,
            Subsystem::Harness,
            None,
            Some(Payload::Elections(weaver_trace::Elections {
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
            })),
        )
        .unwrap();
    author
        .author(
            &mut recorder,
            Kind::TurnStarted,
            Subsystem::Harness,
            Some(&turn),
            None,
        )
        .unwrap();
    author
        .author_message(
            &mut recorder,
            &Message {
                role: Role::User,
                content: vec![ContentBlock::Text {
                    text: "hello".into(),
                }],
            },
            &turn,
        )
        .unwrap()
        .unwrap();
    author
        .author(
            &mut recorder,
            Kind::ModelMeasurement,
            Subsystem::SpuDecoder,
            Some(&turn),
            // The measurement is a spliced blob as of the custody act, the SPU's
            // weights hash carried opaque, and the witness the watch needs is
            // that the assembly never reads it: the secret rides the splice.
            Some(Payload::ModelMeasurement(
                weaver_trace::raw_payload(
                    r#"{"model":"qwen3-4b-instruct","weights_hash":"sha256:secret-witness","input_tokens":[1],"output_tokens":[2],"blocks":[{"label":"turn-delta","start":0,"end":1}],"entropies":[0.5],"timings":{"prefill_ns":"1","decode_ns":"2"}}"#,
                )
                .expect("the measurement blob splices"),
            )),
        )
        .unwrap();

    // The watch's teeth: a fault event whose account happens to be a valid
    // Message. The report shape wraps it now, so a bare message can no longer
    // be a fault payload whole, but the message-shaped content still sits in
    // the record and only the kind filter keeps it out of the prompt - a
    // decode-after-read of the account would admit it, which is exactly the
    // difference the Spec's read-site rule draws.
    let message_shaped = serde_json::to_string(&Message {
        role: Role::User,
        content: vec![ContentBlock::Text {
            text: "smuggled-through-a-fault".into(),
        }],
    })
    .expect("renders");
    let report = weaver_types::FaultReport {
        case: weaver_types::FaultCase::DeviceFaultDuringGeneration,
        account: serde_json::value::RawValue::from_string(message_shaped).expect("parses"),
    };
    author
        .author_fault(&mut recorder, Subsystem::Spu, Some(&turn), &report)
        .expect("fault authored");

    let prompt = assemble(
        recorder.structure().expect("the serving record"),
        "you are an agent",
        &[],
    );
    let rendered = prompt.render();
    assert_eq!(prompt.messages.len(), 1, "only the message kind entered");
    assert!(
        !rendered.contains("secret-witness"),
        "no measurement content reaches the prompt: {rendered}"
    );
    assert!(
        !rendered.contains("model.measurement"),
        "no measurement event at all"
    );
    assert!(
        !rendered.contains("smuggled-through-a-fault"),
        "a message-shaped payload on a non-message kind stays out: {rendered}"
    );
    assert!(rendered.contains("hello"), "the message did enter");
}

/// **Deterministic assembly:** one working structure assembles one prompt,
/// byte-identical across runs.
///
/// Two identical assembles cannot detect an ordering change - a reversed
/// iteration would reverse both renders equally and the comparison would still
/// pass. **The watch is the order assertion below**, which pins the message
/// sequence to what was authored, and the identical-render comparison is the
/// determinism half beside it.
#[test]
fn assembly_is_deterministic() {
    let (author, mut recorder) = author();
    let turn = TurnKey("t-1".to_string());
    author
        .author(
            &mut recorder,
            Kind::Load,
            Subsystem::Harness,
            None,
            Some(Payload::Elections(weaver_trace::Elections {
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
            })),
        )
        .unwrap();
    author
        .author(
            &mut recorder,
            Kind::TurnStarted,
            Subsystem::Harness,
            Some(&turn),
            None,
        )
        .unwrap();
    for text in ["first", "second", "third"] {
        author
            .author_message(
                &mut recorder,
                &Message {
                    role: Role::User,
                    content: vec![ContentBlock::Text { text: text.into() }],
                },
                &turn,
            )
            .unwrap()
            .unwrap();
    }
    let a = assemble(
        recorder.structure().expect("the serving record"),
        "identity",
        &["schema".to_string()],
    );
    let b = assemble(
        recorder.structure().expect("the serving record"),
        "identity",
        &["schema".to_string()],
    );
    assert_eq!(
        a.render(),
        b.render(),
        "the same records assemble the same prompt"
    );
    let order: Vec<&str> = a
        .messages
        .iter()
        .map(|m| match &m.content[0] {
            ContentBlock::Text { text } => text.as_str(),
            _ => "?",
        })
        .collect();
    assert_eq!(
        order,
        ["first", "second", "third"],
        "sequence order, not landing accident"
    );
}

/// The order of parts is the identity prefix, then the message sequence, then
/// the tool schemas.
#[test]
fn prompt_part_order_is_fixed() {
    let (author, mut recorder) = author();
    let turn = TurnKey("t-1".to_string());
    author
        .author(
            &mut recorder,
            Kind::Load,
            Subsystem::Harness,
            None,
            Some(Payload::Elections(weaver_trace::Elections {
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
            })),
        )
        .unwrap();
    author
        .author(
            &mut recorder,
            Kind::TurnStarted,
            Subsystem::Harness,
            Some(&turn),
            None,
        )
        .unwrap();
    author
        .author_message(
            &mut recorder,
            &Message {
                role: Role::User,
                content: vec![ContentBlock::Text {
                    text: "body".into(),
                }],
            },
            &turn,
        )
        .unwrap()
        .unwrap();
    let rendered = assemble(
        recorder.structure().expect("the serving record"),
        "IDENTITY",
        &["SCHEMA".to_string()],
    )
    .render();
    let identity = rendered.find("IDENTITY").expect("identity present");
    let body = rendered.find("body").expect("message present");
    let schema = rendered.find("SCHEMA").expect("schema present");
    assert!(
        identity < body && body < schema,
        "identity, then messages, then schemas"
    );
}

/// A fault is authored by this crate and its payload is carried unchanged:
/// what the reporting organ handed over reaches the record without
/// translation.
#[test]
fn fault_payload_is_carried_unchanged() {
    let (author, mut recorder) = author();
    author
        .author(
            &mut recorder,
            Kind::Load,
            Subsystem::Harness,
            None,
            Some(Payload::Elections(weaver_trace::Elections {
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
            })),
        )
        .unwrap();
    let account = "{\"organ\":\"spu\",\"detail\":\"device unavailable\"}";
    assert!(
        raw_payload(account).is_some(),
        "the account is well-formed octets"
    );
    let report = weaver_types::FaultReport {
        case: weaver_types::FaultCase::DeviceFaultDuringGeneration,
        account: serde_json::value::RawValue::from_string(account.into()).expect("parses"),
    };
    author
        .author_fault(&mut recorder, Subsystem::Spu, None, &report)
        .expect("authored");
    let line = recorder
        .structure()
        .expect("the serving record")
        .by_kind(Kind::Fault)
        .next()
        .expect("the fault landed")
        .line
        .to_string();
    assert!(
        line.contains("\"account\":{\"organ\":\"spu\",\"detail\":\"device unavailable\"}"),
        "the account is spliced unchanged inside the report: {line}"
    );
    assert!(
        line.contains("\"case\":\"device_fault_during_generation\""),
        "the case crosses as its snake_case spelling: {line}"
    );
    assert!(
        line.contains("\"subsystem\":\"spu\""),
        "attributed to the reporting organ"
    );
}

/// A message-kind record that does not decode is counted rather than dropped
/// in silence: the prompt would otherwise carry a hole nothing reports.
#[test]
fn undecodable_message_records_are_counted() {
    let (author, mut recorder) = author();
    let turn = TurnKey("t-1".to_string());
    author
        .author(
            &mut recorder,
            Kind::Load,
            Subsystem::Harness,
            None,
            Some(Payload::Elections(weaver_trace::Elections {
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
            })),
        )
        .unwrap();
    author
        .author(
            &mut recorder,
            Kind::TurnStarted,
            Subsystem::Harness,
            Some(&turn),
            None,
        )
        .unwrap();
    author
        .author_message(
            &mut recorder,
            &Message {
                role: Role::User,
                content: vec![ContentBlock::Text {
                    text: "sound".into(),
                }],
            },
            &turn,
        )
        .unwrap()
        .unwrap();
    // A message-kind event whose payload is not a message: the shape a second
    // producer, or a change to the message model, would introduce.
    author
        .author(
            &mut recorder,
            Kind::MessageUser,
            Subsystem::Harness,
            Some(&turn),
            Some(Payload::Message(
                raw_payload("{\"not\":\"a message\"}").unwrap(),
            )),
        )
        .unwrap();
    let prompt = assemble(
        recorder.structure().expect("the serving record"),
        "identity",
        &[],
    );
    assert_eq!(prompt.messages.len(), 1, "the sound message entered");
    assert_eq!(prompt.undecodable, 1, "the hole is counted, not silent");
}

/// An incomplete prompt does not cross the extension seam: the undecodable
/// count becomes a `fault` event and the seat is not granted, because handing
/// a loop a prompt with a hole would put the loss in the model's context and
/// nowhere else.
///
/// Perturbation: drop the count check from `grant_seat` and the seat is
/// granted with the hole and no fault is authored. Watched under exactly that
/// removal.
#[test]
fn an_undecodable_record_refuses_the_seat_and_authors_a_fault() {
    let prompt_with_hole = {
        let (author, mut recorder) = author();
        let turn = TurnKey("t-1".to_string());
        author
            .author(
                &mut recorder,
                Kind::Load,
                Subsystem::Harness,
                None,
                Some(Payload::Elections(weaver_trace::Elections {
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
                })),
            )
            .unwrap();
        author
            .author(
                &mut recorder,
                Kind::TurnStarted,
                Subsystem::Harness,
                Some(&turn),
                None,
            )
            .unwrap();
        author
            .author(
                &mut recorder,
                Kind::MessageUser,
                Subsystem::Harness,
                Some(&turn),
                Some(Payload::Message(
                    raw_payload("{\"not\":\"a message\"}").unwrap(),
                )),
            )
            .unwrap();
        assemble(
            recorder.structure().expect("the serving record"),
            "identity",
            &[],
        )
    };
    assert_eq!(
        prompt_with_hole.undecodable, 1,
        "the hole is visible to the caller"
    );
    assert!(
        prompt_with_hole.messages.is_empty(),
        "and the message it stood for is absent from the prompt"
    );
}

/// **The identity door writes system messages and refuses every other
/// role.** A door that wrote what it was not built for would launder a bad
/// declaration into a record that reads as well formed, which is the
/// tool-result door's own reasoning applied to the prefix.
///
/// Perturbation: remove the role guard from `author_identity` and the user
/// message below is authored as `message.system`, a role the record then
/// carries under the wrong kind with nothing saying so. Watched under
/// exactly that removal.
///
/// conforms: harness-identity-door-writes-system-only
#[test]
fn the_identity_door_writes_system_only() {
    let (author, mut recorder) = author();
    let prefix = Message {
        role: Role::System,
        content: vec![ContentBlock::Text {
            text: "You are a careful assistant.".into(),
        }],
    };
    author
        .author_identity(&mut recorder, &prefix)
        .expect("the prefix is licensed")
        .expect("and it records");

    let impostor = Message {
        role: Role::User,
        content: vec![ContentBlock::Text {
            text: "not a prefix".into(),
        }],
    };
    let refused = author.author_identity(&mut recorder, &impostor);
    assert!(
        refused.is_err(),
        "a role that is not system is refused at the identity door"
    );

    let lines: Vec<&weaver_trace::Record> = recorder
        .structure()
        .expect("the serving record")
        .iter()
        .collect();
    assert_eq!(lines.len(), 1, "only the prefix reached the record");
    assert_eq!(
        lines[0].kind,
        weaver_trace::Kind::MessageSystem,
        "and it reached it as message.system"
    );
    let value: serde_json::Value =
        serde_json::from_str(lines[0].line.as_ref()).expect("the line parses");
    assert!(
        value.get("turn").is_none(),
        "the prefix belongs to no turn, a prefix preceding every turn there is"
    );
}
