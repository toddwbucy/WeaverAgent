//! The real dormant entry routes a canonical Diagnostic run through replay,
//! paired management, and no-state Leave. Only the SPU/member are scripted.
use super::*;
use std::io::{BufRead, BufReader, Read, Write};
use std::os::fd::FromRawFd;
use weaver_types::{LifecycleAnswer, TokenAnswer, TokenDirective};

#[path = "context_worker_diagnostic_setup_tests.rs"]
mod setup;

pub(super) fn prepare(program: &mut FoundationProgram, root: &std::path::Path) {
    let quote = |s: &str| format!("'{}'", s.replace('\'', "'\\''"));
    let path = root.join("diagnostic-spu");
    std::fs::write(&path, format!(
        "#!/bin/sh\nexport WEAVER_DIAGNOSTIC_CHILD={}\nexec {} --exact flow::context::worker_startup::tests::admission_process::diagnostic::diagnostic_spu_child --nocapture\n",
        quote(root.to_str().unwrap()), quote(std::env::current_exe().unwrap().to_str().unwrap()),
    )).unwrap();
    std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o700)).unwrap();
    if std::env::var("WEAVER_WORKER_STARTUP_CASE")
        .unwrap_or_default()
        .contains("-real-model-")
    {
        real_model::prepare(root, &path);
    }
    program.organs.spu = path;
    for binary in [
        &program.organs.gate,
        program.organs.classify.as_ref().unwrap(),
    ] {
        std::fs::write(
            binary,
            format!(
                "#!/bin/sh\ntouch {}\nexit 91\n",
                quote(root.join("diagnostic-forbidden-organ").to_str().unwrap())
            ),
        )
        .unwrap();
    }
    setup_arms::prepare(program, root);
}

macro_rules! packet {
    ($fd:expr, $value:expr $(,)?) => {{
        let bytes = serde_json::to_vec($value).unwrap();
        assert_eq!(
            nix::sys::socket::send($fd, &bytes, MsgFlags::MSG_NOSIGNAL).unwrap(),
            bytes.len()
        );
    }};
}
macro_rules! take {
    ($fd:expr) => {{
        let mut bytes = vec![0; weaver_types::MAX_ENVELOPE_BYTES];
        let n = recv($fd, &mut bytes, MsgFlags::empty()).unwrap();
        assert!(n > 0);
        serde_json::from_slice(&bytes[..n]).unwrap()
    }};
}

#[path = "context_worker_diagnostic_arm_tests.rs"]
mod setup_arms;

#[path = "context_worker_diagnostic_interrupt_tests.rs"]
mod interrupt;

#[path = "context_worker_diagnostic_member_tests.rs"]
mod real_member;
#[path = "context_worker_diagnostic_model_tests.rs"]
pub(in super::super) mod real_model;

#[test]
fn diagnostic_spu_child() {
    let Ok(root) = std::env::var("WEAVER_DIAGNOSTIC_CHILD") else {
        return;
    };
    let root = std::path::PathBuf::from(root);
    let mode = std::env::var("WEAVER_WORKER_STARTUP_CASE").unwrap();
    std::fs::write(root.join("diagnostic-spawned"), b"spawned\n").unwrap();
    // SAFETY: this actual fork producer placed the two exclusive child ends.
    let lifecycle = unsafe { OwnedFd::from_raw_fd(3) };
    let decode = unsafe { OwnedFd::from_raw_fd(4) };
    if mode.contains("-setup-arm-") {
        setup_arms::spu(&root, &mode, &lifecycle, &decode);
        return;
    }
    let request: OrganEnvelope = take!(lifecycle.as_raw_fd());
    let Payload::Directive(LifecycleDirective::Admit { instruction }) = &request.payload else {
        panic!("Admit first")
    };
    assert!(instruction.classify.is_none());
    assert!(instruction.decoder.refeed_permission);
    let column_expected = mode.ends_with("-record-pressure") || mode.ends_with("-record-failure");
    assert_eq!(
        instruction.decoder.residual_readout_election,
        column_expected
    );
    assert_eq!(instruction.decoder.column_permission, column_expected);
    if mode.ends_with("refused-admit") {
        packet!(
            lifecycle.as_raw_fd(),
            &OrganEnvelope {
                exchange: request.exchange,
                position: Position::Close,
                payload: Payload::Refusal(LifecycleRefusal::DeviceCannotAdmit),
            }
        );
        let end = Instant::now() + Duration::from_secs(3);
        crate::flow::fixture_eof::await_eof_until(decode.as_raw_fd(), end, || {});
        crate::flow::fixture_eof::await_eof_until(lifecycle.as_raw_fd(), end, || {});
        std::fs::write(root.join("diagnostic-refused-closed"), b"no Release\n").unwrap();
        return;
    }
    packet!(
        lifecycle.as_raw_fd(),
        &OrganEnvelope {
            exchange: request.exchange,
            position: Position::Close,
            payload: Payload::Answer(LifecycleAnswer::Admitted),
        },
    );
    let TokenDirective::Open {
        session,
        messages,
        column_ask,
    } = take!(decode.as_raw_fd())
    else {
        panic!("Open follows Admit")
    };
    assert_eq!(messages.len(), 1);
    assert_eq!(column_ask, column_expected);
    if mode.ends_with("refused-open") {
        packet!(decode.as_raw_fd(), &weaver_types::TokenRefusal::OutOfOrder);
        finish_refused_enter(&root, &lifecycle, &decode);
        return;
    }
    let session = if mode.ends_with("invalid-open") {
        weaver_types::SessionId("foreign-session".into())
    } else {
        session
    };
    let rendered = "<system>Read caf\u{e9}.</system>";
    packet!(decode.as_raw_fd(), &TokenAnswer::Opened(weaver_types::Opened {
        session, prefix_tokens: 3, capacity: 16384, generation_limit: 16, terminal_tokens: 1,
        request: serde_json::value::to_raw_value(&serde_json::json!({"rendered":rendered,"template":"fixture"})).unwrap(),
        measurement: serde_json::value::to_raw_value(&serde_json::json!({"model":"art","weights_hash":"h","prefix_token_ids":[41,7,89],"blocks":[{"label":"identity-prefix","start":0,"end":rendered.len()}]})).unwrap(),
    }));
    if mode.ends_with("invalid-open") {
        finish_refused_enter(&root, &lifecycle, &decode);
        return;
    }
    if mode.contains("-interrupt-") {
        interrupt::spu(&root, &mode, &lifecycle, &decode);
        return;
    }
    let TokenDirective::ReFeed {
        turn,
        rendered,
        path,
    } = take!(decode.as_raw_fd())
    else {
        panic!("replay must re-feed")
    };
    assert_eq!(turn.0, "t-1");
    assert_eq!(rendered, "hi");
    assert_eq!(path, vec![3]);
    std::fs::write(root.join("diagnostic-refeed"), b"pending\n").unwrap();
    let end = Instant::now() + Duration::from_secs(5);
    while !root.join("diagnostic-continue").exists() {
        assert!(
            Instant::now() < end,
            "management did not finish during re-feed"
        );
        std::thread::sleep(Duration::from_millis(1));
    }
    let answer: TokenAnswer = serde_json::from_value(serde_json::json!({
        "kind":"re_fed", "body":{
            "emission":"hi","finish":"completed","content":[{"type":"text","text":"hi"}],
            "request":{"rendered":"hi","template":"tmpl","sampling":{"seed":37}},
            "measurement":{"input_tokens":[1,2],"output_tokens":[3],"model":"art","weights_hash":"h"},
            "resident":6,"capacity":64
        }
    })).unwrap();
    packet!(decode.as_raw_fd(), &answer);
    let release: OrganEnvelope = take!(lifecycle.as_raw_fd());
    assert_eq!(
        release.payload,
        Payload::Directive(LifecycleDirective::Release)
    );
    packet!(
        lifecycle.as_raw_fd(),
        &OrganEnvelope {
            exchange: release.exchange,
            position: Position::Close,
            payload: Payload::Answer(LifecycleAnswer::Released),
        },
    );
    std::fs::write(root.join("diagnostic-released"), b"released\n").unwrap();
}

fn finish_refused_enter(root: &std::path::Path, lifecycle: &OwnedFd, decode: &OwnedFd) {
    let release: OrganEnvelope = take!(lifecycle.as_raw_fd());
    assert_eq!(
        release.payload,
        Payload::Directive(LifecycleDirective::Release)
    );
    crate::flow::fixture_eof::await_eof_until(
        decode.as_raw_fd(),
        Instant::now() + Duration::from_secs(3),
        || {},
    );
    packet!(
        lifecycle.as_raw_fd(),
        &OrganEnvelope {
            exchange: release.exchange,
            position: Position::Close,
            payload: Payload::Answer(LifecycleAnswer::Released),
        }
    );
    std::fs::write(
        root.join("diagnostic-released"),
        b"released without replay\n",
    )
    .unwrap();
}

fn observed(peer: &OwnedFd, ordinal: u64, expected: AgentState, end: Instant) {
    send(peer, ordinal, LifecycleDirective::Observe);
    let response = ObservationEnvelope::decode(&receive(peer, end)).unwrap();
    assert_eq!(response.exchange.ordinal, ordinal);
    let ObservationContent::Answer(ObservationAnswer::State {
        state, load, plan, ..
    }) = response.payload
    else {
        panic!("state answer")
    };
    assert_eq!(state, expected);
    assert!(plan.is_none());
    if state == AgentState::Unloaded {
        assert!(load.is_none());
        return;
    }
    let facts = load.unwrap();
    assert_eq!(facts.session.0, "session");
    assert_eq!(facts.run.0, "run");
    assert_eq!(facts.artifact.0, "art");
    assert!(facts.state_member);
    assert_eq!(
        facts.state,
        weaver_types::state_flow::enter::LoadedStateFacts::Diagnostic {
            state_election: weaver_types::StateElection::default()
        }
    );
}

fn malformed_management(
    peer: &OwnedFd,
    source: &EnterEnvelope,
    state: AgentState,
    first_ordinal: u64,
    end: Instant,
) {
    let ordinary = OrganEnvelope {
        exchange: ExchangeId {
            opener: Opener::Admin,
            ordinal: first_ordinal,
        },
        position: Position::Open,
        payload: Payload::Directive(LifecycleDirective::Observe),
    };
    let mut foreign = ordinary.clone();
    foreign.exchange.opener = Opener::Harness;
    let mut continuation = ordinary.clone();
    continuation.position = Position::Continue;
    let mut answer = ordinary.clone();
    answer.payload = Payload::Answer(LifecycleAnswer::AtRest);
    let mut legacy = ordinary.clone();
    legacy.payload = Payload::Directive(LifecycleDirective::Enter {
        payload: Box::new(crate::channel::diagnostic::diagnostic_payload(source).unwrap()),
    });
    for (index, (mut request, reason, handles)) in [
        (foreign, LifecycleRefusal::Malformed, 0),
        (continuation, LifecycleRefusal::Malformed, 0),
        (answer, LifecycleRefusal::Malformed, 0),
        (ordinary.clone(), LifecycleRefusal::Malformed, 1),
        (ordinary, LifecycleRefusal::Malformed, 2),
        (legacy.clone(), LifecycleRefusal::OutOfOrder, 1),
        (legacy, LifecycleRefusal::OutOfOrder, 2),
    ]
    .into_iter()
    .enumerate()
    {
        request.exchange.ordinal += 2 * index as u64;
        let (sink, mut reader) = UnixStream::pair().unwrap();
        let (member, mut watcher) = UnixStream::pair().unwrap();
        let descriptors = [sink.as_raw_fd(), member.as_raw_fd()];
        send_enter(
            peer,
            &serde_json::to_vec(&request).unwrap(),
            &descriptors[..handles],
        );
        drop((sink, member));
        let refusal: serde_json::Value = serde_json::from_slice(&receive(peer, end)).unwrap();
        assert_eq!(
            refusal,
            serde_json::to_value(OrganEnvelope {
                exchange: request.exchange.clone(),
                position: Position::Close,
                payload: Payload::Refusal(reason),
            })
            .unwrap()
        );
        closed_empty(&mut reader, end);
        closed_empty(&mut watcher, end);
        observed(peer, request.exchange.ordinal + 1, state.clone(), end);
    }
}

#[allow(clippy::too_many_arguments)]
pub(super) fn control(
    root: &std::path::Path,
    mode: &str,
    child: &mut std::process::Child,
    mut envelope: EnterEnvelope,
    sink: OwnedFd,
    state: OwnedFd,
    reader: &mut UnixStream,
    member: &mut UnixStream,
    end: Instant,
) {
    let serving_wire = envelope.encode().unwrap();
    let EnterContent::Directive(EnterDirective::Enter { payload }) = &mut envelope.payload;
    payload.binding = EnterBinding::Diagnostic {
        state_election: weaver_types::StateElection::default(),
    };
    if !mode.contains("-setup-arm-") {
        payload.spu_instruction.classify = None;
    } else {
        payload.spu_instruction.classify = Some(weaver_types::ClassifyInstruction {
            model_binding: payload.spu_instruction.decoder.model_binding.clone(),
        });
    }
    payload.spu_instruction.decoder.refeed_permission = true;
    payload.spu_instruction.decoder.model_binding.artifact.0 = "art".into();
    if mode.ends_with("-real-member") || mode.contains("-real-model-") {
        real_member::control(
            root, mode, child, envelope, sink, state, reader, member, end,
        );
        return;
    }
    let wire = envelope.encode().unwrap();
    if mode.contains("-interrupt-") {
        let EnterContent::Directive(EnterDirective::Enter { payload }) = &mut envelope.payload;
        payload.drain_bound = mode.ends_with("-drain-expiry").then_some(1);
        if mode.ends_with("-record-pressure") || mode.ends_with("-record-failure") {
            payload.spu_instruction.decoder.residual_readout_election = true;
            payload.spu_instruction.decoder.column_permission = true;
            payload.spu_instruction.decoder.field_election =
                Some(weaver_types::FieldElection { depth: 40 });
        }
        interrupt::control(
            root, mode, child, &envelope, sink, state, reader, member, end,
        );
        return;
    }
    if mode.contains("-setup-arm-") {
        setup_arms::control(
            root, mode, child, &envelope, sink, state, reader, member, end,
        );
        return;
    }
    if mode.contains("-setup-identity-") {
        setup::identity(
            root, mode, child, &envelope, sink, state, reader, member, end,
        );
        return;
    }
    if mode.ends_with("opener-pressure") {
        opener_pressure(root, child, &envelope, sink, state, reader, member, end);
        return;
    }
    if mode.ends_with("refused-admit")
        || mode.ends_with("refused-open")
        || mode.ends_with("invalid-open")
    {
        refused_enter(
            root, mode, child, &envelope, sink, state, reader, member, end,
        );
        return;
    }
    // A lost pre-load refusal must return the listener to admission too.
    let lost = dial(&root.join("admin.sock"));
    send_enter(&lost, &wire, &[]);
    drop(lost);
    ordinary(root, 919, LifecycleDirective::Observe, true, end);
    let peer = dial(&root.join("admin.sock"));
    // The complete canonical packet without its sink gets the ordinary pre-load
    // refusal. Queue Observe behind it to prove the exact peer returns intact.
    send_enter(&peer, &wire, &[]);
    send(&peer, 920, LifecycleDirective::Observe);
    let refusal: OrganEnvelope = serde_json::from_slice(&receive(&peer, end)).unwrap();
    assert_eq!(refusal.exchange, envelope.exchange);
    assert_eq!(
        refusal.payload,
        Payload::Refusal(LifecycleRefusal::DescriptorsUnusable)
    );
    let unloaded = ObservationEnvelope::decode(&receive(&peer, end)).unwrap();
    assert_eq!(unloaded.exchange.ordinal, 920);
    assert!(matches!(
        unloaded.payload,
        ObservationContent::Answer(ObservationAnswer::State {
            state: AgentState::Unloaded,
            load: None,
            ..
        })
    ));
    no_bytes(reader);
    no_bytes(member);
    member.set_nonblocking(false).unwrap();
    member
        .set_read_timeout(Some(Duration::from_secs(8)))
        .unwrap();
    let mut state_writer = member.try_clone().unwrap();
    std::thread::scope(|scope| {
        let responder = scope.spawn(|| {
            let mut lines = BufReader::new(member).lines();
            let opener = lines.next().unwrap().unwrap();
            assert_eq!(opener, weaver_trace::opener("session", &weaver_trace::Election {
                all_kinds: true,
                keys: Vec::new(),
            }).trim_end());
            let identity = lines.next().unwrap().unwrap();
            assert_eq!(serde_json::from_str::<serde_json::Value>(&identity).unwrap(), serde_json::json!({"ask":{"identity":{}}}));
            writeln!(state_writer, "{}", serde_json::json!({"answer":{"identity":{"messages":[{
                "envelope":{"session":"source","run":"source-run","kind":"message.system","sequence":"1"},
                "pairs":{"role":"system","content":[{"type":"text","text":"Read caf\u{e9}."}]}
            }]}}})).unwrap();
            let replay = lines.next().unwrap().unwrap();
            assert_eq!(serde_json::from_str::<serde_json::Value>(&replay).unwrap(), serde_json::json!({"ask":{"replay":{}}}));
            writeln!(state_writer, "{}", serde_json::json!({"answer":{"replay":{"events":[
                {"envelope":{"kind":"model.request","run":"r-1","turn":"t-1","sequence":"4"},"pairs":{"rendered":"hi","template":"tmpl","sampling":{"seed":37}}},
                {"envelope":{"kind":"model.measurement","run":"r-1","turn":"t-1","sequence":"6"},"pairs":{"input_tokens":[1,2],"output_tokens":[3],"model":"art","weights_hash":"h"}}
            ]}}})).unwrap();
            assert!(lines.next().is_none(), "Diagnostic must neither tee content nor request a save");
        });
        send_enter(&peer, &wire, &[sink.as_raw_fd(), state.as_raw_fd()]);
        drop((sink, state));
        let ready: OrganEnvelope = serde_json::from_slice(&receive(&peer, end)).unwrap();
        assert_eq!(ready.exchange, envelope.exchange);
        assert_eq!(ready.payload, Payload::Answer(LifecycleAnswer::Ready));
        let peer = if mode.ends_with("new-peer") {
            drop(peer);
            dial(&root.join("admin.sock"))
        } else {
            peer
        };
        while !root.join("diagnostic-refeed").exists() {
            assert!(Instant::now() < end && child.try_wait().unwrap().is_none());
            std::thread::sleep(Duration::from_millis(1));
        }
        observed(&peer, 921, AgentState::Active, end);
        // Even a well-formed Serving Enter is OutOfOrder here, never a fallback.
        let (rejected_sink, mut rejected_reader) = UnixStream::pair().unwrap();
        let (rejected_state, mut rejected_member) = UnixStream::pair().unwrap();
        send_enter(
            &peer,
            &serving_wire,
            &[rejected_sink.as_raw_fd(), rejected_state.as_raw_fd()],
        );
        drop((rejected_sink, rejected_state));
        let refused: OrganEnvelope = serde_json::from_slice(&receive(&peer, end)).unwrap();
        assert_eq!(
            refused.payload,
            Payload::Refusal(LifecycleRefusal::OutOfOrder)
        );
        closed_empty(&mut rejected_reader, end);
        closed_empty(&mut rejected_member, end);
        observed(&peer, 926, AgentState::Active, end);
        malformed_management(&peer, &envelope, AgentState::Active, 1000, end);
        std::fs::write(root.join("diagnostic-continue"), b"finish\n").unwrap();
        loop {
            send(&peer, 922, LifecycleDirective::Observe);
            let answer = ObservationEnvelope::decode(&receive(&peer, end)).unwrap();
            if matches!(
                answer.payload,
                ObservationContent::Answer(ObservationAnswer::State {
                    state: AgentState::Idle,
                    ..
                })
            ) {
                break;
            }
            assert!(Instant::now() < end);
            std::thread::sleep(Duration::from_millis(1));
        }
        observed(&peer, 923, AgentState::Idle, end);
        // Repeat reentry at rest: this is the entered service loop, whereas
        // the first refusal is handled inside the pending ReFeed exchange.
        send_enter(&peer, &wire, &[]);
        let refused: OrganEnvelope = serde_json::from_slice(&receive(&peer, end)).unwrap();
        assert_eq!(refused.exchange, envelope.exchange);
        assert_eq!(refused.position, Position::Close);
        assert_eq!(
            refused.payload,
            Payload::Refusal(LifecycleRefusal::OutOfOrder)
        );
        observed(&peer, 927, AgentState::Idle, end);
        malformed_management(&peer, &envelope, AgentState::Idle, 1100, end);
        send(
            &peer,
            924,
            LifecycleDirective::Stop {
                cause: Cause { uid: 0 },
            },
        );
        let stopped: OrganEnvelope = serde_json::from_slice(&receive(&peer, end)).unwrap();
        assert_eq!(stopped.payload, Payload::Answer(LifecycleAnswer::AtRest));
        send(
            &peer,
            925,
            LifecycleDirective::Leave {
                cause: Cause { uid: 0 },
                forced: false,
                rollback: false,
            },
        );
        let left: OrganEnvelope = serde_json::from_slice(&receive(&peer, end)).unwrap();
        assert_eq!(left.exchange.ordinal, 925);
        assert!(matches!(
            left.payload,
            Payload::Answer(LifecycleAnswer::Left {
                no_state: true,
                save_point: None,
                forced: false,
                ..
            })
        ));
        let (status, expired) = crate::test_child::wait_until(child, end).unwrap();
        assert!(!expired, "Diagnostic replay did not finish Leave");
        assert!(status.success());
        responder.join().unwrap();
    });
    assert!(root.join("diagnostic-released").exists());
    assert!(!root.join("diagnostic-forbidden-organ").exists());
    reader.set_nonblocking(false).unwrap();
    reader
        .set_read_timeout(Some(Duration::from_secs(2)))
        .unwrap();
    let mut bytes = String::new();
    reader.read_to_string(&mut bytes).unwrap();
    let rows: Vec<serde_json::Value> = bytes
        .lines()
        .map(|line| serde_json::from_str(line).unwrap())
        .collect();
    assert_eq!(rows.first().unwrap()["kind"], "replay.opened");
    assert_eq!(rows.last().unwrap()["kind"], "replay.closed");
    assert_eq!(
        rows.last().unwrap()["payload"]["outcome"]["kind"],
        "certified"
    );
    assert_eq!(
        rows.iter().filter(|r| r["kind"] == "replay.opened").count(),
        1
    );
    assert!(
        rows.iter()
            .all(|r| r["kind"] != "load" && r["kind"] != "unload" && r["kind"] != "save_point")
    );
}

#[allow(clippy::too_many_arguments)]
fn refused_enter(
    root: &std::path::Path,
    mode: &str,
    child: &mut std::process::Child,
    source: &EnterEnvelope,
    sink: OwnedFd,
    state: OwnedFd,
    reader: &mut UnixStream,
    member: &mut UnixStream,
    end: Instant,
) {
    member.set_nonblocking(false).unwrap();
    member
        .set_read_timeout(Some(Duration::from_secs(8)))
        .unwrap();
    let mut state_writer = member.try_clone().unwrap();
    let peer = dial(&root.join("admin.sock"));
    std::thread::scope(|scope| {
        let responder = scope.spawn(move || {
            let mut input = BufReader::new(member);
            let mut lines = (&mut input).lines();
            let opener = lines.next().unwrap().unwrap();
            assert_eq!(opener, weaver_trace::opener("session", &weaver_trace::Election {
                all_kinds: true, keys: Vec::new(),
            }).trim_end());
            assert_eq!(serde_json::from_str::<serde_json::Value>(&lines.next().unwrap().unwrap()).unwrap(),
                serde_json::json!({"ask":{"identity":{}}}));
            writeln!(state_writer, "{}", serde_json::json!({"answer":{"identity":{"messages":[{
                "envelope":{"session":"source","run":"source-run","kind":"message.system","sequence":"1"},
                "pairs":{"role":"system","content":[{"type":"text","text":"Read caf\u{e9}."}]}
            }]}}})).unwrap();
            let extra = lines.next();
            if extra.is_some() {
                // End the faulty worker's reply wait before failing this
                // instrument; otherwise the refused request has no responder.
                input.get_ref().shutdown(std::net::Shutdown::Both).unwrap();
            }
            assert!(extra.is_none(), "failed Enter must not ask replay, tee content or save");
        });
        send_enter(
            &peer,
            &source.encode().unwrap(),
            &[sink.as_raw_fd(), state.as_raw_fd()],
        );
        drop((sink, state));
        let reason = if mode.ends_with("refused-admit") {
            LifecycleRefusal::OrganRefused {
                organ: weaver_types::RefusingOrgan::Spu,
                reason: Box::new(LifecycleRefusal::DeviceCannotAdmit),
            }
        } else if mode.ends_with("refused-open") {
            LifecycleRefusal::DeviceCannotAdmit
        } else {
            LifecycleRefusal::NoResidency
        };
        let refused: OrganEnvelope = serde_json::from_slice(&receive(&peer, end)).unwrap();
        assert_eq!(
            refused,
            OrganEnvelope {
                exchange: source.exchange.clone(),
                position: Position::Close,
                payload: Payload::Refusal(reason)
            }
        );
        send(&peer, 950, LifecycleDirective::Observe);
        let observed = ObservationEnvelope::decode(&receive(&peer, end)).unwrap();
        assert_eq!(observed.exchange.ordinal, 950);
        assert!(matches!(
            observed.payload,
            ObservationContent::Answer(ObservationAnswer::InTransition { .. })
        ));
        send(
            &peer,
            951,
            LifecycleDirective::Stop {
                cause: Cause { uid: 0 },
            },
        );
        let stopped: OrganEnvelope = serde_json::from_slice(&receive(&peer, end)).unwrap();
        assert_eq!(stopped.exchange.ordinal, 951);
        assert_eq!(
            stopped.payload,
            Payload::Refusal(LifecycleRefusal::OutOfOrder)
        );
        send(
            &peer,
            952,
            LifecycleDirective::Leave {
                cause: Cause { uid: 0 },
                forced: true,
                rollback: true,
            },
        );
        let left: OrganEnvelope = serde_json::from_slice(&receive(&peer, end)).unwrap();
        assert_eq!(
            left.exchange,
            ExchangeId {
                opener: Opener::Admin,
                ordinal: 952
            }
        );
        assert_eq!(left.position, Position::Close);
        assert!(matches!(
            left.payload,
            Payload::Answer(LifecycleAnswer::Left {
                no_state: true,
                save_point: None,
                forced: true,
                ..
            })
        ));
        let (status, expired) = crate::test_child::wait_until(child, end).unwrap();
        assert!(!expired, "refused Diagnostic Enter did not finish rollback");
        assert!(status.success());
        responder.join().unwrap();
    });
    assert!(root.join("diagnostic-spawned").exists());
    if mode.ends_with("refused-admit") {
        assert!(root.join("diagnostic-refused-closed").exists());
        assert!(!root.join("diagnostic-released").exists());
    } else {
        assert!(root.join("diagnostic-released").exists());
    }
    assert!(!root.join("diagnostic-refeed").exists());
    assert!(!root.join("diagnostic-forbidden-organ").exists());
    reader.set_nonblocking(false).unwrap();
    reader
        .set_read_timeout(Some(Duration::from_secs(2)))
        .unwrap();
    let mut trace = Vec::new();
    reader.read_to_end(&mut trace).unwrap();
    assert!(
        trace.is_empty(),
        "refused Diagnostic Enter cannot open a replay bracket"
    );
}

#[allow(clippy::too_many_arguments)]
fn opener_pressure(
    root: &std::path::Path,
    child: &mut std::process::Child,
    source: &EnterEnvelope,
    sink: OwnedFd,
    state: OwnedFd,
    reader: &mut UnixStream,
    member: &mut UnixStream,
    end: Instant,
) {
    nix::sys::socket::setsockopt(&state, nix::sys::socket::sockopt::SndBuf, &4096).unwrap();
    let flags = nix::fcntl::fcntl(&state, nix::fcntl::FcntlArg::F_GETFL).unwrap();
    assert_eq!(
        flags & nix::libc::O_NONBLOCK,
        0,
        "fixture does not fix the worker's end"
    );
    let mut filled = 0;
    loop {
        match nix::sys::socket::send(
            state.as_raw_fd(),
            &[b'x'; 4096],
            MsgFlags::MSG_DONTWAIT | MsgFlags::MSG_NOSIGNAL,
        ) {
            Ok(n) => {
                assert!(n > 0);
                filled += n;
            }
            Err(nix::errno::Errno::EAGAIN) => break,
            Err(error) => panic!("fixture state fill: {error}"),
        }
    }
    let peer = dial(&root.join("admin.sock"));
    send_enter(
        &peer,
        &source.encode().unwrap(),
        &[sink.as_raw_fd(), state.as_raw_fd()],
    );
    drop((sink, state));
    let refused: OrganEnvelope = serde_json::from_slice(&receive(&peer, end)).unwrap();
    assert_eq!(refused.exchange, source.exchange);
    assert_eq!(refused.position, Position::Close);
    assert_eq!(
        refused.payload,
        Payload::Refusal(LifecycleRefusal::DescriptorsUnusable)
    );
    send(
        &peer,
        951,
        LifecycleDirective::Leave {
            cause: Cause { uid: 0 },
            forced: true,
            rollback: true,
        },
    );
    let left: OrganEnvelope = serde_json::from_slice(&receive(&peer, end)).unwrap();
    assert_eq!(
        left.exchange,
        ExchangeId {
            opener: Opener::Admin,
            ordinal: 951
        }
    );
    assert_eq!(left.position, Position::Close);
    assert!(matches!(
        left.payload,
        Payload::Answer(LifecycleAnswer::Left {
            no_state: true,
            save_point: None,
            forced: true,
            ..
        })
    ));
    let (status, expired) = crate::test_child::wait_until(child, end).unwrap();
    assert!(!expired, "failed Diagnostic entry did not finish rollback");
    assert!(status.success());
    assert!(!root.join("diagnostic-spawned").exists());
    assert!(!root.join("diagnostic-forbidden-organ").exists());
    for endpoint in [&*reader, &*member] {
        endpoint.set_nonblocking(false).unwrap();
        endpoint
            .set_read_timeout(Some(Duration::from_secs(2)))
            .unwrap();
    }
    let mut trace = Vec::new();
    reader.read_to_end(&mut trace).unwrap();
    assert!(
        trace.is_empty(),
        "no replay bracket, load or fabricated refusal trace"
    );
    let mut received = Vec::new();
    member.read_to_end(&mut received).unwrap();
    assert_eq!(
        received,
        vec![b'x'; filled],
        "no opener, ask or retry crossed the full seam"
    );
}
