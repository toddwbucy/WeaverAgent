use super::placement::{OpenFacts, PlacementFailure, open_evidence, prepare_identity};
use weaver_spu::decoder::backend::{DecodeFault, TokenId};
use weaver_spu::family;
use weaver_traits::{ContentBlock, Message, Role};
use weaver_types::SessionId;

#[test]
fn every_registered_family_reports_one_whole_identity_render_and_its_actual_tokens() {
    let identity = [Message {
        role: Role::System,
        content: vec![ContentBlock::Text {
            text: "Identity with accented e: \u{e9}.\nSecond line.".into(),
        }],
    }];
    for declaration in family::REGISTRY {
        let renderer = (declaration.renderer)();
        let expected = renderer.render_identity(&identity).unwrap();
        let mut calls = 0;
        let tokens = vec![TokenId(7), TokenId(u32::MAX), TokenId(3)];
        let prepared = prepare_identity(renderer, &identity, |actual| {
            calls += 1;
            assert_eq!(actual, expected, "{}", declaration.family);
            Ok(tokens.clone())
        })
        .unwrap();
        assert_eq!(calls, 1);
        assert_eq!(prepared.tokens, tokens);
        let opened = open_evidence(
            SessionId("s".into()),
            OpenFacts {
                prefix_tokens: 3,
                capacity: 64,
                generation_limit: 11,
                terminal_tokens: 1,
            },
            declaration.template,
            "model",
            "hash",
            &prepared,
        )
        .unwrap();
        assert_eq!(opened.session, SessionId("s".into()));
        assert_eq!(opened.prefix_tokens, 3);
        assert_eq!(opened.capacity, 64);
        assert_eq!(opened.generation_limit, 11);
        assert_eq!(opened.terminal_tokens, 1);
        // Comparing the complete objects also excludes generation-only readings.
        let request: serde_json::Value = serde_json::from_str(opened.request.get()).unwrap();
        let measurement: serde_json::Value =
            serde_json::from_str(opened.measurement.get()).unwrap();
        assert_eq!(
            request,
            serde_json::json!({"rendered":expected,"template":declaration.template})
        );
        assert_eq!(
            measurement,
            serde_json::json!({
                "model":"model","weights_hash":"hash","prefix_token_ids":[7,u32::MAX,3],
                "blocks":[{"label":"identity-prefix","start":0,"end":expected.len()}]
            })
        );
        assert!(opened.request.get().starts_with("{\"rendered\":"));
        assert!(
            opened.measurement.get().starts_with(
                "{\"model\":\"model\",\"weights_hash\":\"hash\",\"prefix_token_ids\":"
            )
        );
        assert!(expected.len() > expected.chars().count());
    }
}

#[test]
fn identity_failure_has_no_evidence_or_second_tokenizer_attempt() {
    let bad = Message {
        role: Role::User,
        content: vec![ContentBlock::ToolCall(weaver_traits::ToolCall {
            name: "calculator".into(),
            arguments: "{}".into(),
        })],
    };
    assert!(matches!(
        prepare_identity(family::qwen2::renderer(), &[bad], |_| panic!(
            "bad identity reached tokenizer"
        )),
        Err(PlacementFailure::Render(_))
    ));
    let fault = DecodeFault::Engine {
        detail: "tokenizer unavailable".into(),
    };
    assert!(
        matches!(prepare_identity(family::qwen2::renderer(), &[], |_| Err(fault.clone())),
        Err(PlacementFailure::Decode(error)) if error == fault)
    );
}

#[test]
fn evidence_rejects_a_count_that_does_not_describe_the_prefill_vector() {
    let prepared =
        prepare_identity(family::qwen2::renderer(), &[], |_| Ok(vec![TokenId(1)])).unwrap();
    let result = open_evidence(
        SessionId("s".into()),
        OpenFacts {
            prefix_tokens: 2,
            capacity: 64,
            generation_limit: 11,
            terminal_tokens: 1,
        },
        "t",
        "m",
        "h",
        &prepared,
    );
    assert!(matches!(result, Err(DecodeFault::Engine { .. })));
}

#[test]
fn open_evidence_escapes_all_identities_and_keeps_an_empty_numeric_token_array() {
    let prepared = super::placement::PreparedIdentity {
        rendered: String::new(),
        tokens: vec![],
    };
    let opened = open_evidence(
        SessionId("s".into()),
        OpenFacts {
            prefix_tokens: 0,
            capacity: 0,
            generation_limit: 0,
            terminal_tokens: 1,
        },
        "template\"\\\n",
        "model\"\\\n",
        "hash\"\\\n",
        &prepared,
    )
    .unwrap();
    assert_eq!(
        opened.request.get(),
        r#"{"rendered":"","template":"template\"\\\n"}"#
    );
    assert_eq!(
        opened.measurement.get(),
        r#"{"model":"model\"\\\n","weights_hash":"hash\"\\\n","prefix_token_ids":[],"blocks":[{"label":"identity-prefix","start":0,"end":0}]}"#
    );
}

fn bounded_decode_pair() -> (
    weaver_spu::channel::DecodeSocket,
    weaver_spu::channel::DecodeSocket,
) {
    use nix::sys::socket::{AddressFamily, SockFlag, SockType, setsockopt, socketpair};
    use nix::sys::time::TimeVal;

    let (sender, receiver) = socketpair(
        AddressFamily::Unix,
        SockType::SeqPacket,
        None,
        SockFlag::SOCK_CLOEXEC,
    )
    .unwrap();
    for end in [&sender, &receiver] {
        setsockopt(
            end,
            nix::sys::socket::sockopt::ReceiveTimeout,
            &TimeVal::new(5, 0),
        )
        .unwrap();
        setsockopt(
            end,
            nix::sys::socket::sockopt::SendTimeout,
            &TimeVal::new(5, 0),
        )
        .unwrap();
    }
    (
        weaver_spu::channel::decode_from_owned(sender),
        weaver_spu::channel::decode_from_owned(receiver),
    )
}

/// Size the complete production encoding, including the adjacent tag, both
/// boxes and escaped newlines. This supplies synthetic preparation evidence;
/// it does not claim that a real family/backend admits this large identity.
fn opened_with_encoded_size(wanted: usize) -> weaver_types::TokenAnswer {
    use super::placement::PreparedIdentity;
    use weaver_types::TokenAnswer;

    let mut escaped_bytes = wanted - 512;
    for _ in 0..4 {
        let mut rendered = "\n".repeat(escaped_bytes / 2);
        if !escaped_bytes.is_multiple_of(2) {
            rendered.push('x');
        }
        let prepared = PreparedIdentity {
            rendered,
            tokens: vec![TokenId(u32::MAX)],
        };
        let answer = TokenAnswer::Opened(
            open_evidence(
                SessionId("bounded-open".into()),
                OpenFacts {
                    prefix_tokens: 1,
                    capacity: 64,
                    generation_limit: 11,
                    terminal_tokens: 1,
                },
                "family",
                "model",
                "hash",
                &prepared,
            )
            .unwrap(),
        );
        let actual = serde_json::to_vec(&answer).unwrap().len();
        if actual == wanted {
            return answer;
        }
        escaped_bytes = if actual < wanted {
            escaped_bytes + (wanted - actual)
        } else {
            escaped_bytes.checked_sub(actual - wanted).unwrap()
        };
    }
    panic!("could not size the complete Opened encoding");
}

#[test]
fn complete_opened_at_the_decode_bound_crosses_without_truncating_either_box() {
    use weaver_types::{DECODE_MESSAGE_BOUND, TokenAnswer};

    let answer = opened_with_encoded_size(DECODE_MESSAGE_BOUND);
    let expected = serde_json::to_vec(&answer).unwrap();
    assert_eq!(expected.len(), 8 * 1024 * 1024);
    let TokenAnswer::Opened(opened) = &answer else {
        unreachable!()
    };
    let request: serde_json::Value = serde_json::from_str(opened.request.get()).unwrap();
    assert!(request["rendered"].as_str().unwrap().len() < DECODE_MESSAGE_BOUND / 2);
    let (sender, receiver) = bounded_decode_pair();
    let sending = std::thread::spawn(move || super::send_answer(&sender, &answer));
    let received = receiver.recv_octets();
    // A broken send bound or incomplete series must not strand the other end.
    drop(receiver);
    let sent = sending.join().unwrap();
    assert_eq!(sent, Ok(()));
    let received = received.unwrap();
    assert_eq!(received, expected);
    assert!(matches!(
        serde_json::from_slice::<TokenAnswer>(&received).unwrap(),
        TokenAnswer::Opened(opened) if opened.session.0 == "bounded-open"
    ));
}

#[test]
fn complete_opened_over_the_decode_bound_sends_no_preamble_or_partial_evidence() {
    use nix::errno::Errno;
    use nix::sys::socket::{MsgFlags, recv};
    use std::os::fd::AsRawFd;
    use weaver_spu::channel::ChannelFault;
    use weaver_types::DECODE_MESSAGE_BOUND;

    let answer = opened_with_encoded_size(DECODE_MESSAGE_BOUND + 1);
    assert_eq!(
        serde_json::to_vec(&answer).unwrap().len(),
        8 * 1024 * 1024 + 1
    );
    let (sender, receiver) = bounded_decode_pair();
    let sent = super::send_answer(&sender, &answer);
    let mut first = [0_u8; 128];
    assert_eq!(
        recv(
            receiver.as_fd().as_raw_fd(),
            &mut first,
            MsgFlags::MSG_DONTWAIT
        ),
        Err(Errno::EAGAIN),
        "an oversized Opened must not begin even a segment preamble"
    );
    assert_eq!(
        sent,
        Err(ChannelFault::Truncated {
            bound: DECODE_MESSAGE_BOUND
        })
    );
}

#[test]
fn shared_open_corpus_preserves_box_bytes_through_the_spu_answer_serializer() {
    use weaver_types::TokenAnswer;

    let cases: Vec<serde_json::Value> = serde_json::from_str(include_str!(
        "../../weaver-types/tests/fixtures/decode-open-evidence.json"
    ))
    .unwrap();
    let (sender, receiver) = bounded_decode_pair();
    for case in cases {
        let name = case["name"].as_str().unwrap();
        let decoded = serde_json::from_str::<TokenAnswer>(case["wire"].as_str().unwrap());
        assert_eq!(
            decoded.is_ok(),
            case["outer_valid"].as_bool().unwrap(),
            "{name}"
        );
        let Ok(answer @ TokenAnswer::Opened(_)) = decoded else {
            continue;
        };
        let TokenAnswer::Opened(opened) = &answer else {
            unreachable!()
        };
        let request = opened.request.get().to_owned();
        let measurement = opened.measurement.get().to_owned();
        // These are serializer inputs, not certified producer observations.
        // Invalid nested semantics remain opaque to the shared wire codec.
        super::send_answer(&sender, &answer).unwrap();
        let returned = receiver.recv_octets().unwrap();
        let TokenAnswer::Opened(returned) = serde_json::from_slice(&returned).unwrap() else {
            panic!("{name} changed answer kind")
        };
        assert_eq!(returned.request.get(), request, "{name} request");
        assert_eq!(
            returned.measurement.get(),
            measurement,
            "{name} measurement"
        );
        assert_eq!(TokenAnswer::Opened(returned), answer, "{name}");
    }
}
