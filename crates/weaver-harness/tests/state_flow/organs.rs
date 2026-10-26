//! Scripted language/model/gate peers. Only ordinary protocol traffic crosses
//! their sockets. They are not a classifier or model qualification artifact.
use nix::errno::Errno;
use nix::sys::socket::{MsgFlags, recv, send};
use std::os::fd::{AsRawFd, FromRawFd, OwnedFd};
use std::path::PathBuf;
use std::sync::OnceLock;
use std::time::{Duration, Instant};
use weaver_types::state_flow as s;
use weaver_types::{LifecycleAnswer, LifecycleDirective, OrganEnvelope, Payload, Position};

pub(super) fn templates() -> &'static [s::TemplateSpec] {
    static VALUES: OnceLock<Vec<s::TemplateSpec>> = OnceLock::new();
    VALUES.get_or_init(|| {
        vec![
            s::TemplateSpec {
                template_id: "fixture.first".into(),
                kind: "loop.instruction".into(),
                parts: vec![s::TemplatePart::Literal("Read file.".into())],
            },
            s::TemplateSpec {
                template_id: "mark.begin_task".into(),
                kind: "state.mark".into(),
                parts: vec![s::TemplatePart::Literal("Read task.".into())],
            },
        ]
    })
}

macro_rules! packet {
    ($fd:expr, $value:expr $(,)?) => {{
        let bytes = serde_json::to_vec($value).unwrap();
        assert_eq!(send($fd, &bytes, MsgFlags::MSG_NOSIGNAL), Ok(bytes.len()));
    }};
}

fn receive(fd: i32, end: Instant) -> Option<Vec<u8>> {
    let mut bytes = vec![0; weaver_types::MAX_ENVELOPE_BYTES];
    loop {
        match recv(fd, &mut bytes, MsgFlags::MSG_DONTWAIT) {
            Ok(0) => return None,
            Ok(n) => {
                bytes.truncate(n);
                return Some(bytes);
            }
            Err(Errno::EAGAIN) => (),
            Err(error) => panic!("scripted organ receive: {error}"),
        }
        assert!(Instant::now() < end, "scripted organ watchdog");
        std::thread::sleep(Duration::from_millis(1));
    }
}

fn classifier(fd: i32) {
    let end = Instant::now() + Duration::from_secs(35);
    while let Some(bytes) = receive(fd, end) {
        // Spec 02 CHANNEL header. These small fixtures fit one fragment; a
        // larger fixture must explicitly implement and check reassembly.
        assert_eq!(bytes[0], 1);
        let tag = bytes[1];
        let id = u64::from_be_bytes(bytes[2..10].try_into().unwrap());
        assert_eq!(
            u64::from_be_bytes(bytes[10..18].try_into().unwrap()),
            (bytes.len() - 26) as u64
        );
        assert_eq!(&bytes[18..26], &[0; 8]);
        let (tag, payload) = if tag == 1 {
            assert_eq!(id, 0);
            let bootstrap: s::Ste100Bootstrap = serde_json::from_slice(&bytes[26..]).unwrap();
            let digest = super::member::digest(bootstrap.scheme_text.as_bytes());
            (
                4,
                serde_json::to_vec(&s::Ste100Ready {
                    protocol: s::STE100_PROTOCOL.into(),
                    scheme_id: format!("ste100:{digest}"),
                    artifact_sha256: digest,
                    checker_artifact_sha256: bootstrap.instruction.model_artifact_sha256,
                    instances: bootstrap.instruction.instances,
                    max_wire_bytes: bootstrap.instruction.max_wire_bytes,
                })
                .unwrap(),
            )
        } else {
            assert_eq!(tag, 2);
            let request: s::Ste100Request = serde_json::from_slice(&bytes[26..]).unwrap();
            let bindings = if matches!(
                request.target,
                s::Ste100Target::Input { .. } | s::Ste100Target::Source(_)
            ) {
                assert!(request.text.starts_with("Read ") && request.text.ends_with('.'));
                let length = request.text.len() as u64;
                vec![s::Binding {
                    statement_span: s::Span {
                        start: 0.into(),
                        end: length.into(),
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
                            end: (length - 1).into(),
                        },
                    }],
                }]
            } else {
                vec![]
            };
            (
                3,
                serde_json::to_vec(&s::Ste100Reply {
                    request_id: request.request_id,
                    target: request.target,
                    scheme_id: request.scheme_id,
                    result: s::Ste100Result::Accepted { bindings },
                })
                .unwrap(),
            )
        };
        let mut answer = vec![1, tag];
        answer.extend_from_slice(&id.to_be_bytes());
        answer.extend_from_slice(&(payload.len() as u64).to_be_bytes());
        answer.extend_from_slice(&0u64.to_be_bytes());
        answer.extend(payload);
        assert_eq!(send(fd, &answer, MsgFlags::MSG_NOSIGNAL), Ok(answer.len()));
    }
}

fn gate(fd: i32) {
    let end = Instant::now() + Duration::from_secs(35);
    let root = PathBuf::from(std::env::var_os("WEAVER_EXTERNAL_STATE_ROOT").unwrap());
    let raise: OrganEnvelope = serde_json::from_slice(&receive(fd, end).unwrap()).unwrap();
    let Payload::Directive(LifecycleDirective::Raise { socket, .. }) = &raise.payload else {
        panic!("gate requires Raise")
    };
    let _listener = std::os::unix::net::UnixListener::bind(socket).unwrap();
    packet!(
        fd,
        &OrganEnvelope {
            exchange: raise.exchange,
            position: Position::Close,
            payload: Payload::Answer(LifecycleAnswer::GateReady),
        },
    );
    if std::env::var("WEAVER_EXTERNAL_STATE_HELD").unwrap() == "yes" {
        packet!(
            fd,
            &OrganEnvelope {
                exchange: weaver_types::ExchangeId {
                    opener: weaver_types::Opener::Gate,
                    ordinal: 101,
                },
                position: Position::Open,
                payload: Payload::Frame(weaver_types::TurnFrame::carry_from(
                    br#"{"text":"Read caf\u00e9."}"#,
                    42,
                )),
            },
        );
    }
    while let Some(bytes) = receive(fd, end) {
        let message: OrganEnvelope = serde_json::from_slice(&bytes).unwrap();
        match message.payload {
            Payload::Frame(frame) => {
                let close: serde_json::Value =
                    serde_json::from_slice(&frame.octets().unwrap()).unwrap();
                assert_eq!(close["kind"], "stopped");
                assert_eq!(message.position, Position::Close);
                std::fs::write(root.join("world-close.json"), &bytes).unwrap();
            }
            Payload::Directive(LifecycleDirective::Lower) => {
                std::fs::write(root.join("ready-rollback-lower.json"), &bytes).unwrap();
                packet!(
                    fd,
                    &OrganEnvelope {
                        exchange: message.exchange,
                        position: Position::Close,
                        payload: Payload::Answer(LifecycleAnswer::GateStopped),
                    },
                );
                return;
            }
            other => panic!("unexpected gate command: {other:?}"),
        }
    }
}

fn spu(lifecycle: i32, decode: i32) {
    let end = Instant::now() + Duration::from_secs(35);
    let admit: OrganEnvelope = serde_json::from_slice(&receive(lifecycle, end).unwrap()).unwrap();
    assert!(matches!(
        admit.payload,
        Payload::Directive(LifecycleDirective::Admit { .. })
    ));
    packet!(
        lifecycle,
        &OrganEnvelope {
            exchange: admit.exchange,
            position: Position::Close,
            payload: Payload::Answer(LifecycleAnswer::Admitted),
        },
    );
    let open: weaver_types::TokenDirective =
        serde_json::from_slice(&receive(decode, end).unwrap()).unwrap();
    let weaver_types::TokenDirective::Open {
        session, messages, ..
    } = open
    else {
        panic!("Open required")
    };
    assert_eq!(messages.len(), 1);
    let rendered = "<system>Read item.</system>";
    packet!(decode, &weaver_types::TokenAnswer::Opened(weaver_types::Opened {
        session, prefix_tokens: 3, capacity: 16384, generation_limit: 16, terminal_tokens: 1,
        request: serde_json::value::to_raw_value(&serde_json::json!({"rendered":rendered,"template":"synthetic"})).unwrap(),
        measurement: serde_json::value::to_raw_value(&serde_json::json!({"model":"synthetic","weights_hash":"synthetic","prefix_token_ids":[41,7,89],"blocks":[{"label":"identity-prefix","start":0,"end":rendered.len()}]})).unwrap(),
    }));
    let release: OrganEnvelope = serde_json::from_slice(&receive(lifecycle, end).unwrap()).unwrap();
    assert!(matches!(
        release.payload,
        Payload::Directive(LifecycleDirective::Release)
    ));
    assert!(
        receive(decode, end).is_none(),
        "decode retires before Release"
    );
    packet!(
        lifecycle,
        &OrganEnvelope {
            exchange: release.exchange,
            position: Position::Close,
            payload: Payload::Answer(LifecycleAnswer::Released),
        },
    );
}

pub(super) fn run(kind: &str) {
    // SAFETY: the public startup's actual fork producer maps these descriptors
    // exclusively into this separately exec'd fixture process.
    let first = unsafe { OwnedFd::from_raw_fd(3) };
    match kind {
        "checker" => classifier(first.as_raw_fd()),
        "gate" => {
            // SAFETY: the gate's inherited tools cgroup descriptor is unique.
            let tools = unsafe { OwnedFd::from_raw_fd(4) };
            assert_eq!(
                nix::sys::statfs::fstatfs(&tools).unwrap().filesystem_type(),
                nix::sys::statfs::CGROUP2_SUPER_MAGIC
            );
            gate(first.as_raw_fd());
        }
        "spu" => {
            // SAFETY: the decoder's inherited endpoint is unique.
            let decode = unsafe { OwnedFd::from_raw_fd(4) };
            spu(first.as_raw_fd(), decode.as_raw_fd());
        }
        _ => panic!("unknown scripted organ"),
    }
}
