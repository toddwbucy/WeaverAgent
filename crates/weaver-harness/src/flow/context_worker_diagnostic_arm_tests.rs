//! Rollback crosses real held organ exchanges, with no constructed owner bits.
use super::*;

#[path = "context_worker_diagnostic_pressure_tests.rs"]
mod pressure;

pub(super) fn prepare(program: &mut FoundationProgram, root: &std::path::Path) {
    if !std::env::var("WEAVER_WORKER_STARTUP_CASE")
        .is_ok_and(|mode| mode.contains("-setup-arm-classifier"))
    {
        return;
    }
    let quote = |s: &str| format!("'{}'", s.replace('\'', "'\\''"));
    std::fs::write(program.organs.classify.as_ref().unwrap(), format!(
        "#!/bin/sh\nexport WEAVER_DIAGNOSTIC_CHILD={}\nexec {} --exact flow::context::worker_startup::tests::admission_process::diagnostic::setup_arms::diagnostic_classifier_child --nocapture\n",
        quote(root.to_str().unwrap()), quote(std::env::current_exe().unwrap().to_str().unwrap()),
    )).unwrap();
}

fn eof(fd: i32) {
    crate::flow::fixture_eof::await_eof_until(fd, Instant::now() + Duration::from_secs(5), || {});
}

fn no_packet(fd: i32) {
    let mut byte = [0];
    assert_eq!(
        recv(fd, &mut byte, MsgFlags::MSG_DONTWAIT),
        Err(nix::errno::Errno::EAGAIN)
    );
}

fn marker(root: &std::path::Path, name: &str) {
    std::fs::write(root.join(name), std::process::id().to_string()).unwrap();
}

fn wait_marker(root: &std::path::Path, name: &str, mut step: impl FnMut()) {
    let seconds = if std::env::var("WEAVER_WORKER_STARTUP_CASE")
        .is_ok_and(|mode| mode.ends_with("-deadline"))
    {
        200
    } else {
        8
    };
    let end = Instant::now() + Duration::from_secs(seconds);
    while !root.join(name).exists() {
        step();
        assert!(
            Instant::now() < end,
            "fixture marker {name} was not released"
        );
        std::thread::sleep(Duration::from_millis(1));
    }
}

fn opened(session: weaver_types::SessionId) -> TokenAnswer {
    let rendered = "<system>Read caf\u{e9}.</system>";
    TokenAnswer::Opened(weaver_types::Opened {
        session, prefix_tokens: 3, capacity: 16384, generation_limit: 16, terminal_tokens: 1,
        request: serde_json::value::to_raw_value(&serde_json::json!({"rendered":rendered,"template":"fixture"})).unwrap(),
        measurement: serde_json::value::to_raw_value(&serde_json::json!({"model":"art","weights_hash":"h","prefix_token_ids":[41,7,89],"blocks":[{"label":"identity-prefix","start":0,"end":rendered.len()}]})).unwrap(),
    })
}

pub(super) fn spu(root: &std::path::Path, mode: &str, lifecycle: &OwnedFd, decode: &OwnedFd) {
    marker(root, "diagnostic-setup-spu-pid");
    let request: OrganEnvelope = take!(lifecycle.as_raw_fd());
    assert_eq!(request.exchange.opener, Opener::Harness);
    assert_eq!(request.position, Position::Open);
    assert!(
        matches!(&request.payload, Payload::Directive(LifecycleDirective::Admit { instruction })
        if instruction.classify.is_some() && instruction.decoder.refeed_permission)
    );
    let mut reply = OrganEnvelope {
        exchange: request.exchange.clone(),
        position: Position::Close,
        payload: Payload::Answer(LifecycleAnswer::Admitted),
    };
    if mode.contains("-arm-admit") {
        marker(root, "diagnostic-setup-held");
        wait_marker(root, "diagnostic-setup-settle", || {
            // Rollback owes this Admit. Neither a premature Release nor Open
            // (nor an invented decode Cancel) can precede its terminal.
            no_packet(lifecycle.as_raw_fd());
            no_packet(decode.as_raw_fd());
        });
        if mode.ends_with("-lost") {
            nix::sys::socket::shutdown(lifecycle.as_raw_fd(), nix::sys::socket::Shutdown::Both)
                .unwrap();
            eof(decode.as_raw_fd());
            marker(root, "diagnostic-setup-closed-without-release");
            return;
        }
        if mode.ends_with("-refused") {
            reply.payload = Payload::Refusal(LifecycleRefusal::DeviceCannotAdmit);
        } else if mode.ends_with("-wrong-opener") {
            reply.exchange.opener = Opener::Admin;
        } else if mode.ends_with("-wrong-ordinal") {
            reply.exchange.ordinal += 1;
        } else if mode.ends_with("-wrong-position") {
            reply.position = Position::Continue;
        } else if mode.ends_with("-wrong-payload") {
            reply.payload = Payload::Answer(LifecycleAnswer::Released);
        }
        packet!(lifecycle.as_raw_fd(), &reply);
        if mode.ends_with("-refused") || mode.contains("-wrong-") {
            eof(decode.as_raw_fd());
            eof(lifecycle.as_raw_fd());
            marker(root, "diagnostic-setup-closed-without-release");
            return;
        }
    } else {
        packet!(lifecycle.as_raw_fd(), &reply);
        let TokenDirective::Open {
            session, messages, ..
        } = take!(decode.as_raw_fd())
        else {
            panic!("Open follows the confirmed Admit")
        };
        assert_eq!(messages.len(), 1);
        marker(root, "diagnostic-setup-open");
        if mode.contains("-arm-classifier") {
            packet!(decode.as_raw_fd(), &opened(session));
        } else {
            if mode.ends_with("-partial") {
                let mut partial = serde_json::to_value(opened(session)).unwrap();
                partial["body"]["request"]["padding"] =
                    serde_json::json!("x".repeat(weaver_types::MAX_ENVELOPE_BYTES));
                let bytes = serde_json::to_vec(&partial).unwrap();
                assert!(bytes.len() > weaver_types::MAX_ENVELOPE_BYTES);
                packet!(
                    decode.as_raw_fd(),
                    &weaver_types::SegmentPreamble {
                        segments: bytes.len().div_ceil(weaver_types::MAX_ENVELOPE_BYTES) as u64,
                        bytes: bytes.len() as u64,
                    }
                );
                let segment = &bytes[..weaver_types::MAX_ENVELOPE_BYTES];
                assert_eq!(
                    nix::sys::socket::send(decode.as_raw_fd(), segment, MsgFlags::MSG_NOSIGNAL)
                        .unwrap(),
                    segment.len()
                );
            }
            marker(root, "diagnostic-setup-held");
        }
    }
    // Ending Open is a channel close, never a turn-scoped Cancel or a new Open.
    eof(decode.as_raw_fd());
    let release: OrganEnvelope = take!(lifecycle.as_raw_fd());
    assert_eq!(release.exchange.opener, Opener::Harness);
    assert_eq!(release.exchange.ordinal, request.exchange.ordinal + 1);
    assert_eq!(release.position, Position::Open);
    assert_eq!(
        release.payload,
        Payload::Directive(LifecycleDirective::Release)
    );
    marker(root, "diagnostic-setup-release");
    if mode.ends_with("-deadline") {
        // Admit settled after part of the interval was spent. Withhold Release
        // through the original cap. A new cleanup interval fails the parent.
        crate::flow::fixture_eof::await_eof_until(
            lifecycle.as_raw_fd(),
            Instant::now() + Duration::from_secs(160),
            || {},
        );
        return;
    }
    wait_marker(root, "diagnostic-setup-settle", || {
        no_packet(lifecycle.as_raw_fd())
    });
    let mut exchange = release.exchange;
    if mode.ends_with("-release-wrong") {
        exchange.ordinal += 1;
    }
    packet!(
        lifecycle.as_raw_fd(),
        &OrganEnvelope {
            exchange,
            position: Position::Close,
            payload: Payload::Answer(LifecycleAnswer::Released),
        }
    );
    if mode.ends_with("-reap-held") {
        marker(root, "diagnostic-setup-release-answered");
        wait_marker(root, "diagnostic-setup-exit", || {});
    }
    marker(root, "diagnostic-setup-released");
}

#[test]
fn diagnostic_classifier_child() {
    let Ok(root) = std::env::var("WEAVER_DIAGNOSTIC_CHILD") else {
        return;
    };
    let root = std::path::PathBuf::from(root);
    // SAFETY: the actual elected classifier fork owns this one child endpoint.
    let channel = unsafe { OwnedFd::from_raw_fd(3) };
    marker(&root, "diagnostic-setup-classifier-pid");
    marker(&root, "diagnostic-setup-held");
    eof(channel.as_raw_fd());
    marker(&root, "diagnostic-setup-classifier-closed");
}

fn transition(peer: &OwnedFd, ordinal: u64, end: Instant) {
    send(peer, ordinal, LifecycleDirective::Observe);
    let response = ObservationEnvelope::decode(&receive(peer, end)).unwrap();
    assert_eq!(response.exchange.ordinal, ordinal);
    assert!(matches!(
        response.payload,
        ObservationContent::Answer(ObservationAnswer::InTransition { .. })
    ));
}

#[allow(clippy::too_many_arguments)]
pub(super) fn control(
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
    let mut original = Some(dial(&root.join("admin.sock")));
    send_enter(
        original.as_ref().unwrap(),
        &source.encode().unwrap(),
        &[sink.as_raw_fd(), state.as_raw_fd()],
    );
    drop((sink, state));
    member.set_nonblocking(false).unwrap();
    member
        .set_read_timeout(Some(Duration::from_secs(3)))
        .unwrap();
    let mut input = BufReader::new(&mut *member);
    let mut line = String::new();
    input.read_line(&mut line).unwrap();
    assert_eq!(
        line.trim_end(),
        weaver_trace::opener(
            "session",
            &weaver_trace::Election {
                all_kinds: true,
                keys: Vec::new(),
            }
        )
        .trim_end()
    );
    line.clear();
    input.read_line(&mut line).unwrap();
    assert_eq!(
        serde_json::from_str::<serde_json::Value>(&line).unwrap(),
        serde_json::json!({"ask":{"identity":{}}})
    );
    assert!(input.buffer().is_empty());
    drop(input);
    writeln!(member, "{}", serde_json::json!({"answer":{"identity":{"messages":[{
        "envelope":{"session":"source","run":"source-run","kind":"message.system","sequence":"1"},
        "pairs":{"role":"system","content":[{"type":"text","text":"Read caf\u{e9}."}]}
    }]}}})).unwrap();
    wait_marker(root, "diagnostic-setup-held", || {
        assert!(child.try_wait().unwrap().is_none())
    });
    no_bytes(reader);
    if mode.ends_with("-pressure") {
        pressure::fill(original.as_ref().unwrap(), root, end);
    }
    if mode.ends_with("-eof") {
        drop(original.take());
    }
    let silent = dial(&root.join("admin.sock"));
    let probe = dial(&root.join("admin.sock"));
    transition(&probe, 970, end);
    send(
        &probe,
        971,
        LifecycleDirective::Stop {
            cause: Cause { uid: 0 },
        },
    );
    let refused: OrganEnvelope = serde_json::from_slice(&receive(&probe, end)).unwrap();
    assert_eq!(refused.exchange.ordinal, 971);
    assert_eq!(
        refused.payload,
        Payload::Refusal(LifecycleRefusal::OutOfOrder)
    );
    let new = (!mode.ends_with("-same")).then(|| dial(&root.join("admin.sock")));
    let rollback = new.as_ref().or(original.as_ref()).unwrap();
    let rollback_began = Instant::now();
    send(
        rollback,
        972,
        LifecycleDirective::Leave {
            cause: Cause { uid: 0 },
            forced: true,
            rollback: true,
        },
    );
    if mode.ends_with("-pressure") {
        transition(&probe, 973, end);
    } else if let Some(peer) = &original {
        let refusal: OrganEnvelope = serde_json::from_slice(&receive(peer, end)).unwrap();
        assert_eq!(refusal.exchange, source.exchange);
        assert_eq!(refusal.position, Position::Close);
        assert_eq!(
            refusal.payload,
            Payload::Refusal(LifecycleRefusal::NoResidency)
        );
    } else {
        // A separate Observe proves the accepted rollback remains responsive
        // even though there is no Enter reply left to wait on.
        transition(rollback, 973, end);
    }
    transition(&probe, 974, end);
    send(
        &probe,
        975,
        LifecycleDirective::JoinLeave {
            cause: Cause { uid: 0 },
        },
    );
    let joined: OrganEnvelope = serde_json::from_slice(&receive(&probe, end)).unwrap();
    assert_eq!(joined.exchange.ordinal, 975);
    assert_eq!(
        joined.payload,
        Payload::Refusal(LifecycleRefusal::OutOfOrder)
    );
    no_packet(rollback.as_raw_fd());
    if mode.ends_with("-silent-leave") {
        // A new silent dial during the pending rollback must expire without
        // holding cleanup or borrowing a fresh operation interval.
        let silent_leave = dial(&root.join("admin.sock"));
        crate::flow::fixture_eof::await_eof_until(
            silent_leave.as_raw_fd(),
            Instant::now()
                + Duration::from_millis(crate::state::ANSWER_BOUND_MS)
                + Duration::from_secs(2),
            || {},
        );
        transition(&probe, 978, end);
        no_packet(rollback.as_raw_fd());
        assert!(child.try_wait().unwrap().is_none());
    }
    let accepted_by = Instant::now();
    if mode.ends_with("-deadline") {
        while accepted_by.elapsed() < Duration::from_secs(20) {
            transition(&probe, 977, end);
            no_packet(rollback.as_raw_fd());
            std::thread::sleep(Duration::from_millis(100));
        }
    }
    marker(root, "diagnostic-setup-settle");
    if mode.ends_with("-reap-held") {
        wait_marker(root, "diagnostic-setup-release-answered", || {
            assert!(child.try_wait().unwrap().is_none())
        });
        transition(&probe, 976, end);
        no_packet(rollback.as_raw_fd());
        marker(root, "diagnostic-setup-exit");
    }
    if mode.ends_with("-release-wrong") || mode.ends_with("-deadline") {
        let cap = if mode.ends_with("-deadline") {
            accepted_by
                + Duration::from_millis(weaver_types::LEAVE_BOUND_MS)
                + Duration::from_secs(5)
        } else {
            end
        };
        crate::flow::fixture_eof::await_eof_until(rollback.as_raw_fd(), cap, || {});
        if mode.ends_with("-deadline") {
            assert!(
                rollback_began.elapsed() >= Duration::from_millis(weaver_types::LEAVE_BOUND_MS)
            );
        }
    } else {
        let left: OrganEnvelope = serde_json::from_slice(&receive(rollback, end)).unwrap();
        assert_eq!(left.exchange.ordinal, 972);
        assert_eq!(left.position, Position::Close);
        assert_eq!(
            left.payload,
            Payload::Answer(LifecycleAnswer::Left {
                forced: true,
                no_state: true,
                save_point: None,
            })
        );
    }
    if mode.ends_with("-pressure") {
        // Left has already arrived on the other peer while the Enter reply
        // was blocked. Retained replies now drain exactly once.
        pressure::drain(original.as_ref().unwrap(), source, end);
    }
    drop(silent);
    let (status, expired) = crate::test_child::wait_until(child, end).unwrap();
    assert!(!expired, "setup rollback did not finish");
    assert_eq!(
        status.success(),
        !mode.ends_with("-release-wrong") && !mode.ends_with("-deadline")
    );
    let pid = std::fs::read_to_string(root.join("diagnostic-setup-spu-pid")).unwrap();
    assert!(
        !std::path::Path::new(&format!("/proc/{}", pid.trim())).exists(),
        "owned SPU must be reaped before the worker ends"
    );
    assert!(!root.join("diagnostic-refeed").exists());
    assert!(!root.join("diagnostic-forbidden-organ").exists());
    assert_eq!(
        root.join("diagnostic-setup-open").exists(),
        !mode.contains("-arm-admit")
    );
    let refused_admit = mode.contains("-arm-admit")
        && (mode.ends_with("-refused") || mode.ends_with("-lost") || mode.contains("-wrong-"));
    assert_eq!(
        root.join("diagnostic-setup-release").exists(),
        !refused_admit
    );
    assert_eq!(
        root.join("diagnostic-setup-closed-without-release")
            .exists(),
        refused_admit
    );
    let classifier = mode.contains("-arm-classifier");
    assert_eq!(
        root.join("diagnostic-setup-classifier-pid").exists(),
        classifier
    );
    assert_eq!(
        root.join("diagnostic-setup-classifier-closed").exists(),
        classifier
    );
    closed_empty(reader, end);
    closed_empty(member, end);
}
