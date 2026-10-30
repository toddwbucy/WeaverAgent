//! Entered replay management crosses the actual worker and record sink.
use super::*;

#[path = "context_worker_diagnostic_close_tests.rs"]
pub(super) mod closing;
#[path = "context_worker_diagnostic_fault_tests.rs"]
mod faults;
#[path = "context_worker_diagnostic_idle_tests.rs"]
mod idle;
#[path = "context_worker_diagnostic_join_tests.rs"]
mod joined;
#[path = "context_worker_diagnostic_pending_tests.rs"]
mod pending;
#[path = "context_worker_diagnostic_record_pressure_tests.rs"]
pub(super) mod record_pressure;
#[path = "context_worker_diagnostic_send_pressure_tests.rs"]
mod send_pressure;

pub(super) fn spu(root: &std::path::Path, mode: &str, lifecycle: &OwnedFd, decode: &OwnedFd) {
    std::fs::write(
        root.join("diagnostic-interrupt-spu-pid"),
        std::process::id().to_string(),
    )
    .unwrap();
    if mode.contains("-preload-") || mode.ends_with("-before-send-death") {
        pending::spu(root, mode, decode);
    }
    if !mode.ends_with("preload")
        && !mode.contains("-before-send")
        && !mode.ends_with("-ticket-leave")
        && (!mode.contains("-preload-") || mode.ends_with("-resume"))
    {
        let request = if mode.contains("-send-") {
            send_pressure::receive(root, mode, decode)
        } else {
            take!(decode.as_raw_fd())
        };
        let TokenDirective::ReFeed { turn, path, .. } = request else {
            panic!("the recorded generation opens")
        };
        assert_eq!(
            turn.0,
            if mode.contains("-close-") {
                send_pressure::turn_before_send()
            } else {
                "t-1".into()
            }
        );
        let pressure = mode.ends_with("-record-pressure") || mode.ends_with("-record-failure");
        assert_eq!(path, if pressure { vec![3, 4, 5] } else { vec![3] });
        if pressure {
            record_pressure::column(decode, root);
        }
        std::fs::write(root.join("diagnostic-refeed"), b"active\n").unwrap();
        if mode.ends_with("-active-death") {
            pending::spu(root, mode, decode);
        }
        if mode.contains("-close-") {
            packet!(decode.as_raw_fd(), &completed());
            if mode.ends_with("-close-duplicate") {
                closing::duplicate(root, decode);
            }
            if mode.ends_with("-close-report") {
                pending::spu(root, mode, decode);
            }
            if mode.ends_with("-close-stop") || mode.ends_with("-close-report") {
                // A Stop during the close answers AtRest and the pass
                // continues: the edit and the next turn are still owed.
                closing::finish_source(decode);
            }
        } else if mode.ends_with("-graceful-complete")
            || mode.contains("-idle-complete-")
            || mode.ends_with("-later-complete")
            || mode.contains("-release-")
            || mode.ends_with("-preload-report-resume")
        {
            let end = Instant::now() + Duration::from_secs(5);
            while !root.join("diagnostic-complete").exists() {
                assert!(Instant::now() < end);
                std::thread::sleep(Duration::from_millis(1));
            }
            let mut bytes = [0; 4096];
            assert_eq!(
                recv(decode.as_raw_fd(), &mut bytes, MsgFlags::MSG_DONTWAIT),
                Err(nix::errno::Errno::EAGAIN),
                "graceful drain sends no Cancel"
            );
            packet!(decode.as_raw_fd(), &completed());
        } else {
            let cancel: TokenDirective = take!(decode.as_raw_fd());
            assert_eq!(cancel, TokenDirective::Cancel { turn: turn.clone() });
            std::fs::write(root.join("diagnostic-cancelled"), b"one Cancel\n").unwrap();
            if mode.contains("-stop-then-") {
                joined::terminal(root);
            }
            if mode.contains("-interrupt-fail-")
                || mode.ends_with("-graceful-silent")
                || mode.ends_with("-send-delayed-silent")
            {
                faults::spu(mode, &turn, decode);
            }
            if mode.ends_with("-complete-race") {
                packet!(decode.as_raw_fd(), &completed());
            } else if mode.ends_with("-refeed-refused") {
                // The ReFeed's refusal crosses the Cancel: it is the ReFeed's
                // terminal, and the Cancel still answers at rest.
                packet!(decode.as_raw_fd(), &weaver_types::TokenRefusal::OutOfOrder);
            } else {
                packet!(
                    decode.as_raw_fd(),
                    &TokenAnswer::RefeedInterrupted {
                        turn,
                        consumed: u64::from(pressure),
                        resident: if pressure { 7 } else { 5 },
                        capacity: 64,
                    }
                );
            }
            packet!(decode.as_raw_fd(), &TokenAnswer::AtRest);
        }
    }
    if mode.ends_with("-later-complete") {
        closing::finish_source(decode);
    }
    if mode.contains("-idle-") {
        idle::spu(root, mode, decode);
    }
    let release: OrganEnvelope = take!(lifecycle.as_raw_fd());
    assert_eq!(
        release.payload,
        Payload::Directive(LifecycleDirective::Release)
    );
    if mode.ends_with("-release-eof")
        || mode.ends_with("-release-refused")
        || mode.ends_with("-release-write-failure")
    {
        if !mode.ends_with("-release-refused") {
            nix::sys::socket::shutdown(lifecycle.as_raw_fd(), nix::sys::socket::Shutdown::Both)
                .unwrap();
        } else {
            packet!(
                lifecycle.as_raw_fd(),
                &OrganEnvelope {
                    exchange: release.exchange,
                    position: Position::Close,
                    payload: Payload::Refusal(LifecycleRefusal::OutOfOrder),
                }
            );
        }
        // The worker owns termination and reap after an unconfirmed release.
        std::thread::sleep(Duration::from_secs(90));
        panic!("the failed release left its owned SPU alive");
    }
    packet!(
        lifecycle.as_raw_fd(),
        &OrganEnvelope {
            exchange: release.exchange,
            position: Position::Close,
            payload: Payload::Answer(LifecycleAnswer::Released),
        }
    );
    crate::flow::fixture_eof::await_eof_until(
        decode.as_raw_fd(),
        Instant::now() + Duration::from_secs(3),
        || {},
    );
    std::fs::write(root.join("diagnostic-released"), b"released\n").unwrap();
}

fn completed() -> TokenAnswer {
    serde_json::from_value(serde_json::json!({
        "kind":"re_fed", "body":{
            "emission":"hi","finish":"completed","content":[{"type":"text","text":"hi"}],
            "request":{"rendered":"hi","template":"tmpl","sampling":{"seed":37}},
            "measurement":{"input_tokens":[1,2],"output_tokens":[3],"model":"art","weights_hash":"h"},
            "resident":6,"capacity":64
        }
    })).unwrap()
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
    let pressure = mode.ends_with("-record-pressure") || mode.ends_with("-record-failure");
    let record_failure = mode.ends_with("-record-failure");
    let before_send = mode.contains("-before-send");
    let closing = mode.contains("-close-");
    // A Stop during the close continues the pass (Spec 6.2).
    let closing_stop = mode.ends_with("-close-stop") || mode.ends_with("-close-report");
    let two_turns = mode.ends_with("-later-complete") || closing_stop;
    let ticket = mode.ends_with("-ticket-leave");
    let followup = mode.contains("-stop-then-");
    let followup_graceful = mode.ends_with("-stop-then-graceful");
    let source_turn = if before_send || closing {
        send_pressure::turn_before_send()
    } else {
        "t-1".into()
    };
    if pressure || before_send || closing || ticket {
        record_pressure::prepare(&sink);
    }
    let mut early_bytes = String::new();
    let mut held_prefix = String::new();
    let timed = mode.ends_with("-silent") || mode.ends_with("-no-rest");
    let long_graceful = mode.ends_with("-graceful-silent");
    let failing =
        mode.contains("-interrupt-fail-") || long_graceful || mode.contains("-send-") && timed;
    let parked_source = mode.ends_with("preload") || mode.contains("-preload-");
    let preload = parked_source && !mode.ends_with("-resume");
    let preload_death = mode.contains("-preload-death-");
    let join = mode.ends_with("-graceful-join") || long_graceful;
    let clean = mode.ends_with("-graceful-complete");
    let certified = mode.contains("-idle-complete-")
        || mode.ends_with("-later-complete")
        || mode.contains("-release-")
        || mode.ends_with("-preload-report-resume");
    let drain = mode.ends_with("-drain-expiry");
    let rollback = mode.ends_with("-rollback");
    let leave_first = join
        || clean
        || drain
        || rollback
        || mode.ends_with("-forced-leave")
        || mode.ends_with("-close-leave")
        || ticket;
    member.set_nonblocking(false).unwrap();
    member
        .set_read_timeout(Some(Duration::from_secs(if timed {
            180
        } else if mode.ends_with("-death") || preload_death {
            20
        } else {
            8
        })))
        .unwrap();
    let mut writer = member.try_clone().unwrap();
    let member_wake = member.try_clone().unwrap();
    let peer = dial(&root.join("admin.sock"));
    std::thread::scope(|scope| {
        let mut trace_drain = None;
        let mut leave_peer = None;
        let responder = scope.spawn(|| {
            let mut lines = BufReader::new(member).lines();
            assert!(lines.next().unwrap().unwrap().contains("session"));
            assert_eq!(
                serde_json::from_str::<serde_json::Value>(&lines.next().unwrap().unwrap()).unwrap(),
                serde_json::json!({"ask":{"identity":{}}})
            );
            writeln!(writer, "{}", serde_json::json!({"answer":{"identity":{"messages":[{
                "envelope":{"session":"source","run":"source-run","kind":"message.system","sequence":"1"},
                "pairs":{"role":"system","content":[{"type":"text","text":"Read caf\u{e9}."}]}
            }]}}})).unwrap();
            assert_eq!(
                serde_json::from_str::<serde_json::Value>(&lines.next().unwrap().unwrap()).unwrap(),
                serde_json::json!({"ask":{"replay":{}}})
            );
            if parked_source {
                std::fs::write(root.join("diagnostic-preload"), b"parked\n").unwrap();
                if mode.ends_with("-resume") || preload_death {
                    pending::answer(root);
                }
            }
            if !parked_source || mode.ends_with("-resume") || preload_death {
                writeln!(writer, "{}", serde_json::json!({"answer":{"replay":{"events":closing::events(mode, &source_turn, pressure)}}})).unwrap();
            }
            let tail = lines.next();
            assert!(
                tail.is_none() || preload_death && matches!(&tail, Some(Err(error)) if error.kind() == std::io::ErrorKind::ConnectionReset),
                "no retry, later ask or save: {tail:?}"
            );
        });
        let _unwind = faults::UnwindMember(member_wake);
        send_enter(
            &peer,
            &source.encode().unwrap(),
            &[sink.as_raw_fd(), state.as_raw_fd()],
        );
        drop((sink, state));
        let ready: OrganEnvelope = serde_json::from_slice(&receive(&peer, end)).unwrap();
        assert_eq!(ready.exchange, source.exchange);
        assert_eq!(ready.payload, Payload::Answer(LifecycleAnswer::Ready));
        if pressure {
            early_bytes = record_pressure::preamble(reader, end);
        }
        if before_send {
            early_bytes = send_pressure::hold_turn_start(reader, end);
        }
        if closing {
            early_bytes = closing::held(reader, end);
        }
        if ticket {
            early_bytes = closing::ticket(reader, end);
            send(
                &peer,
                970,
                LifecycleDirective::Stop {
                    cause: Cause { uid: 41 },
                },
            );
            let stopped: OrganEnvelope = serde_json::from_slice(&receive(&peer, end)).unwrap();
            assert_eq!(stopped.exchange.ordinal, 970);
            assert_eq!(stopped.position, Position::Close);
            assert_eq!(stopped.payload, Payload::Answer(LifecycleAnswer::AtRest));
        }
        let marker = if parked_source {
            "diagnostic-preload"
        } else {
            "diagnostic-refeed"
        };
        while !before_send && !closing && !ticket && !root.join(marker).exists() {
            assert!(Instant::now() < end && child.try_wait().unwrap().is_none());
            std::thread::sleep(Duration::from_millis(1));
        }
        if mode.ends_with("-before-send-death") || mode.ends_with("-active-death") {
            pending::death(root, child, &peer, reader, &mut early_bytes, end);
            responder.join().unwrap();
            return;
        }
        if mode.contains("-preload-") {
            pending::preload(root, mode, &peer, reader, &mut early_bytes, end);
        }
        if pressure {
            held_prefix = record_pressure::held(root, reader, &peer, end);
        }
        let began = Instant::now();
        if leave_first {
            send(
                &peer,
                972,
                LifecycleDirective::Leave {
                    cause: Cause { uid: 71 },
                    forced: !join && !clean && !drain && !ticket,
                    rollback,
                },
            );
            if closing || ticket {
                closing::pending(root, &peer, end, true);
                let mut stream = reader.try_clone().unwrap();
                stream.set_nonblocking(false).unwrap();
                stream
                    .set_read_timeout(Some(Duration::from_secs(8)))
                    .unwrap();
                trace_drain = Some(scope.spawn(move || {
                    let mut bytes = String::new();
                    stream.read_to_string(&mut bytes).unwrap();
                    bytes
                }));
            }
            if join || clean || drain {
                send(&peer, 971, LifecycleDirective::Observe);
                let observation = ObservationEnvelope::decode(&receive(&peer, end)).unwrap();
                assert_eq!(observation.exchange.ordinal, 971);
                assert!(matches!(
                    observation.payload,
                    ObservationContent::Answer(ObservationAnswer::InTransition { .. })
                ));
                if long_graceful {
                    // No declared drain bound: remain responsive longer than
                    // the cancellation constant without inventing a Cancel.
                    while began.elapsed()
                        < Duration::from_millis(weaver_types::DIAGNOSTIC_CANCEL_BOUND_MS + 2_000)
                    {
                        assert!(!root.join("diagnostic-cancelled").exists());
                        send(&peer, 977, LifecycleDirective::Observe);
                        let answer = ObservationEnvelope::decode(&receive(&peer, end)).unwrap();
                        assert_eq!(answer.exchange.ordinal, 977);
                        assert!(matches!(
                            answer.payload,
                            ObservationContent::Answer(ObservationAnswer::InTransition { .. })
                        ));
                        assert!(child.try_wait().unwrap().is_none());
                        std::thread::sleep(Duration::from_millis(100));
                    }
                }
                if join {
                    let forced_at = Instant::now();
                    send(
                        &peer,
                        973,
                        LifecycleDirective::JoinLeave {
                            cause: Cause { uid: 72 },
                        },
                    );
                    if long_graceful {
                        faults::timed_finish(root, mode, child, &peer, end, forced_at);
                        responder.join().unwrap();
                        return;
                    }
                } else if clean {
                    std::fs::write(root.join("diagnostic-complete"), b"finish\n").unwrap();
                }
            }
        } else {
            if certified {
                std::fs::write(root.join("diagnostic-complete"), b"finish\n").unwrap();
                idle::await_certificate(reader, &mut early_bytes);
            } else {
                send(
                    &peer,
                    970,
                    LifecycleDirective::Stop {
                        cause: Cause { uid: 41 },
                    },
                );
                if followup {
                    leave_peer = Some(joined::leave(root, &peer, end, !followup_graceful));
                }
                if mode.contains("-send-") {
                    send_pressure::stopped(root, &peer, end, mode);
                }
                if pressure || before_send || closing {
                    if closing {
                        closing::pending(root, &peer, end, false);
                        if mode.ends_with("-close-report") {
                            std::fs::write(
                                root.join("diagnostic-pending-action"),
                                b"close is held\n",
                            )
                            .unwrap();
                            early_bytes.push_str(&pending::held_report(root, &peer, reader, end));
                        }
                        if mode.ends_with("-close-duplicate") {
                            std::fs::write(
                                root.join("diagnostic-close-duplicate"),
                                b"close is pending\n",
                            )
                            .unwrap();
                            faults::finish(root, child, &peer, end);
                            responder.join().unwrap();
                            return;
                        }
                        if closing_stop {
                            // Draining the held close lets it record; the
                            // continued pass then certifies the later turn.
                            idle::await_certificate(reader, &mut early_bytes);
                        }
                    } else if before_send {
                        send_pressure::stopped_before_send(root, &peer, end);
                    } else {
                        record_pressure::cancelled(root, &peer, end);
                    }
                    if record_failure {
                        reader.shutdown(std::net::Shutdown::Both).unwrap();
                        faults::finish(root, child, &peer, end);
                        responder.join().unwrap();
                        return;
                    }
                    let mut stream = reader.try_clone().unwrap();
                    stream.set_nonblocking(false).unwrap();
                    stream
                        .set_read_timeout(Some(Duration::from_secs(8)))
                        .unwrap();
                    trace_drain = Some(scope.spawn(move || {
                        let mut bytes = String::new();
                        stream.read_to_string(&mut bytes).unwrap();
                        bytes
                    }));
                }
                if failing {
                    if timed {
                        faults::timed_finish(root, mode, child, &peer, end, began);
                    } else {
                        faults::finish(root, child, &peer, end);
                    }
                    responder.join().unwrap();
                    return;
                }
                let response: OrganEnvelope = serde_json::from_slice(&receive(
                    &peer,
                    end.min(Instant::now() + Duration::from_secs(3)),
                ))
                .unwrap();
                assert_eq!(response.exchange.ordinal, 970);
                assert_eq!(response.position, Position::Close);
                assert_eq!(
                    response.payload,
                    Payload::Answer(if preload || closing {
                        LifecycleAnswer::AtRest
                    } else {
                        LifecycleAnswer::TurnAborted {
                            turn: weaver_types::TurnKey(source_turn.clone()),
                        }
                    })
                );
            }
            if !followup {
                observed(&peer, 971, AgentState::Idle, end);
            }
            if mode.contains("-idle-")
                && idle::control(root, mode, child, &peer, reader, &mut early_bytes, end)
            {
                responder.join().unwrap();
                return;
            }
            if !followup {
                if mode.ends_with("-release-write-failure") {
                    reader.shutdown(std::net::Shutdown::Both).unwrap();
                }
                send(
                    &peer,
                    972,
                    LifecycleDirective::Leave {
                        cause: Cause { uid: 71 },
                        forced: !mode.contains("-idle-")
                            && !mode.contains("-release-")
                            && !mode.contains("-preload-")
                            && !mode.ends_with("-close-report"),
                        rollback: false,
                    },
                );
                if mode.ends_with("-release-write-failure") {
                    faults::finish(root, child, &peer, end);
                    responder.join().unwrap();
                    return;
                }
            }
        }
        for ordinal in if join { vec![972, 973] } else { vec![972] } {
            let left: OrganEnvelope =
                serde_json::from_slice(&receive(leave_peer.as_ref().unwrap_or(&peer), end))
                    .unwrap();
            assert_eq!(left.exchange.ordinal, ordinal);
            assert_eq!(
                left.payload,
                Payload::Answer(LifecycleAnswer::Left {
                    no_state: true,
                    forced: !(clean
                        || mode.ends_with("-report")
                        || ticket
                        || followup_graceful
                        || mode.ends_with("-release-confirmed")
                        || mode.contains("-preload-") && !preload_death),
                    save_point: None,
                })
            );
        }
        if drain {
            assert!(
                began.elapsed() >= Duration::from_millis(900),
                "the declared one-second drain is not one millisecond"
            );
        }
        let (status, expired) = crate::test_child::wait_until(child, end).unwrap();
        assert!(!expired && status.success());
        responder.join().unwrap();
        if let Some(drain) = trace_drain {
            early_bytes.push_str(&drain.join().unwrap());
        }
    });
    if record_failure {
        assert!(!held_prefix.contains("\"kind\":\"turn.closed\""));
        assert!(!held_prefix.contains("\"kind\":\"replay.closed\""));
        return;
    }
    if mode.ends_with("-before-send-death") || mode.ends_with("-active-death") {
        return;
    }
    if mode.ends_with("-idle-invalid")
        || mode.ends_with("-idle-write-failed")
        || mode.ends_with("-close-duplicate")
        || mode.ends_with("-release-write-failure")
    {
        return;
    }
    assert_eq!(
        root.join("diagnostic-released").exists(),
        !failing
            && !mode.ends_with("-death")
            && !mode.ends_with("-release-eof")
            && !mode.ends_with("-release-refused")
            && !preload_death
    );
    reader.set_nonblocking(false).unwrap();
    reader
        .set_read_timeout(Some(Duration::from_secs(2)))
        .unwrap();
    let mut bytes = early_bytes;
    reader.read_to_string(&mut bytes).unwrap();
    let rows: Vec<serde_json::Value> = bytes
        .lines()
        .map(|line| serde_json::from_str(line).unwrap())
        .collect();
    if mode.contains("-preload-") || mode.ends_with("-close-report") {
        pending::rows(&rows, mode);
    }
    if mode.contains("-release-") {
        let pid = std::fs::read_to_string(root.join("diagnostic-interrupt-spu-pid")).unwrap();
        assert!(!std::path::Path::new(&format!("/proc/{}", pid.trim())).exists());
        let faults: Vec<_> = rows.iter().filter(|row| row["kind"] == "fault").collect();
        assert_eq!(faults.len(), usize::from(mode.ends_with("-release-eof")));
        if let Some(fault) = faults.first() {
            assert_eq!(fault["payload"]["case"], "organ_death_observed");
            assert_eq!(fault["payload"]["account"]["organ"], "spu");
            assert!(fault.get("turn").is_none());
            assert_eq!(
                rows.last(),
                Some(*fault),
                "the release death is recorded before Left"
            );
        }
    }
    assert_eq!(rows.first().unwrap()["kind"], "replay.opened");
    assert_eq!(
        rows.iter()
            .filter(|row| row["kind"] == "replay.closed")
            .count(),
        usize::from(certified || ticket || closing_stop)
    );
    let closes: Vec<_> = rows
        .iter()
        .filter(|row| row["kind"] == "turn.closed")
        .collect();
    assert_eq!(
        closes.len(),
        if two_turns {
            2
        } else {
            usize::from(!preload && !timed && !ticket)
        }
    );
    assert_eq!(
        rows.iter().filter(|r| r["kind"] == "elision").count(),
        usize::from(two_turns)
    );
    assert_eq!(
        rows.iter().filter(|r| r["kind"] == "turn.started").count(),
        if two_turns {
            2
        } else {
            usize::from(!preload && !ticket)
        }
    );
    if before_send {
        let starts: Vec<_> = rows
            .iter()
            .filter(|row| row["kind"] == "turn.started")
            .collect();
        assert_eq!(starts.len(), 1);
        assert_eq!(starts[0]["turn"], source_turn);
    }
    if let Some(close) = closes.first() {
        assert_eq!(close["turn"], source_turn);
        assert_eq!(
            close["payload"]["close"],
            if clean || certified || closing {
                "clean"
            } else {
                "stopped"
            }
        );
        if !clean && !certified && !closing {
            assert_eq!(
                close["payload"]["reason"],
                if failing {
                    "fault"
                } else if leave_first || followup && !followup_graceful {
                    "unload"
                } else {
                    "directive"
                }
            );
            if !failing {
                assert_eq!(
                    close["payload"]["cause"]["uid"],
                    if leave_first || followup && !followup_graceful {
                        71
                    } else {
                        41
                    }
                );
            }
        }
    }
    if two_turns {
        assert_eq!(closes[1]["turn"], "t-2");
        assert_eq!(closes[1]["payload"]["close"], "clean");
    }
    if ticket {
        let closed = rows
            .iter()
            .find(|row| row["kind"] == "replay.closed")
            .unwrap();
        assert_eq!(
            closed["payload"],
            serde_json::json!({"outcome":{"kind":"abandoned","reason":{"kind":"identity_refused","detail":format!("the record replays {} and the load declared art", closing::foreign_artifact())}}})
        );
    }
    if pressure {
        record_pressure::rows(&rows);
    }
    if mode.ends_with("-refeed-refused") {
        // The refusal is the ReFeed's terminal, recorded inside its turn.
        let refusals: Vec<_> = rows.iter().filter(|r| r["kind"] == "refusal").collect();
        assert_eq!(refusals.len(), 1);
        assert_eq!(refusals[0]["turn"], source_turn);
        assert_eq!(
            refusals[0]["payload"],
            serde_json::to_value(weaver_types::RefusalRecord::Decode {
                asked: weaver_types::TokenAsk::ReFeed,
                refusal: weaver_types::TokenRefusal::OutOfOrder,
            })
            .unwrap()
        );
        let at = |kind: &str| rows.iter().position(|r| r["kind"] == kind).unwrap();
        assert!(at("refusal") < at("turn.closed"));
    }
    for kind in ["model.request", "model.output", "model.measurement"] {
        assert_eq!(
            rows.iter().filter(|row| row["kind"] == kind).count(),
            if two_turns {
                2
            } else {
                usize::from(clean || certified || closing || mode.ends_with("-complete-race"))
            },
            "only a truthful complete terminal supplies {kind}"
        );
    }
}
