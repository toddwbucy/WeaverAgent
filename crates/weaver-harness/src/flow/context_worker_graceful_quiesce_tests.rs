//! Actual worker Session, root management and gate sockets. Classifier/model
//! peers and the filing member are declared doubles, not qualification inputs.
use super::*;
use weaver_types::{Cause, LifecycleAsk, LifecycleRefusal, RefusalRecord};

pub(in crate::flow::context) fn is_mode(mode: &str) -> bool {
    mode.contains("-session-graceful-quiesce-")
}

fn pending_frame(index: usize) -> OrganEnvelope {
    let mut input = frame(index, index == 2);
    if index == 1 {
        input.payload = Payload::Frame(weaver_types::TurnFrame::carry_from(
            br#"{"role":"system","text":"queued seed"}"#,
            42,
        ));
    }
    input
}

pub(in crate::flow::context) fn gate(mode: &str, root: &Path, fd: i32) {
    let active = mode.ends_with("active") || mode.ends_with("queued");
    let queued = mode.ends_with("queued");
    let mut bytes = vec![0; weaver_types::MAX_ENVELOPE_BYTES];
    if !mode.ends_with("empty") {
        packet!(fd, &pending_frame(0));
    }
    if queued {
        packet!(fd, &pending_frame(1));
    }
    if active {
        let n = recv(fd, &mut bytes, MsgFlags::empty()).unwrap();
        let close: OrganEnvelope = serde_json::from_slice(&bytes[..n]).unwrap();
        assert_eq!(close.exchange, frame(0, false).exchange);
        let Payload::Frame(answer) = close.payload else {
            panic!("turn answer before Quiesce")
        };
        let answer: serde_json::Value = serde_json::from_slice(&answer.octets().unwrap()).unwrap();
        assert_eq!(answer["kind"], "answered");
        std::fs::write(root.join("quiesce-turn-close"), &bytes[..n]).unwrap();
    }
    let n = recv(fd, &mut bytes, MsgFlags::empty()).unwrap();
    let quiesce: OrganEnvelope = serde_json::from_slice(&bytes[..n]).unwrap();
    assert_eq!(quiesce.position, Position::Open);
    assert_eq!(quiesce.exchange.opener, Opener::Harness);
    assert_eq!(quiesce.exchange.ordinal, 2);
    assert!(matches!(
        quiesce.payload,
        Payload::Directive(LifecycleDirective::Quiesce)
    ));
    std::fs::write(root.join("quiesce-request"), &bytes[..n]).unwrap();
    if mode.ends_with("dead") {
        return;
    }
    if mode.ends_with("empty") {
        packet!(fd, &pending_frame(0));
    }
    if !queued {
        packet!(
            fd,
            &pending_frame(if mode.ends_with("duplicate") { 0 } else { 1 })
        );
    }
    packet!(fd, &pending_frame(2));
    // GateQuiesced orders after all admitted frames, but need not wait for
    // the world responses. The worker must keep those obligations separately.
    if !mode.ends_with("expiry") && !mode.ends_with("join") && !mode.ends_with("listener") {
        wait_file(root, "quiesce-ack-release");
        let mut exchange = quiesce.exchange.clone();
        if mode.ends_with("wrong") {
            exchange.ordinal += 1;
        }
        packet!(
            fd,
            &OrganEnvelope {
                exchange,
                position: Position::Close,
                payload: Payload::Answer(LifecycleAnswer::GateQuiesced),
            }
        );
    }
    for index in usize::from(active)..3 {
        let n = recv(fd, &mut bytes, MsgFlags::empty()).unwrap_or(0);
        if n == 0 {
            return;
        }
        let close: OrganEnvelope = serde_json::from_slice(&bytes[..n]).unwrap();
        assert_eq!(close.exchange, pending_frame(index).exchange);
        assert_eq!(close.position, Position::Close);
        let Payload::Frame(answer) = close.payload else {
            panic!("unloading world refusal")
        };
        let answer: serde_json::Value = serde_json::from_slice(&answer.octets().unwrap()).unwrap();
        assert_eq!(
            answer,
            serde_json::json!({"kind":"refused", "reason":"the agent is unloading"})
        );
        std::fs::write(root.join(format!("quiesce-close-{index}")), &bytes[..n]).unwrap();
    }
    std::fs::write(root.join("quiesce-all-closed"), b"yes").unwrap();
    if mode.ends_with("listener") {
        wait_file(root, "quiesce-ack-release");
        packet!(
            fd,
            &OrganEnvelope {
                exchange: quiesce.exchange,
                position: Position::Close,
                payload: Payload::Answer(LifecycleAnswer::GateQuiesced),
            }
        );
    }
    // No repeated Quiesce, Lower, generation or dispatch is elected here.
    assert_eq!(recv(fd, &mut bytes, MsgFlags::empty()).unwrap_or(0), 0);
}

pub(super) fn exercise(
    serving: FoundationServing<'_>,
    member: &mut Member,
    reader: &mut UnixStream,
    socket: &Path,
    root: &Path,
    mode: &str,
) {
    let active = mode.ends_with("active") || mode.ends_with("queued");
    let checking = mode.ends_with("checking");
    let invalid = mode.ends_with("wrong") || mode.ends_with("duplicate");
    let forced = mode.ends_with("expiry") || mode.ends_with("join") || mode.ends_with("dead");
    let empty = mode.ends_with("empty");
    let dead = mode.ends_with("dead");
    let mut guard = SessionGuard(Some(Session::begin(serving, &FAMILY, CutMethod::Flush)));
    let done = AtomicBool::new(false);
    reader.set_nonblocking(true).unwrap();
    let (primary, bytes) = std::thread::scope(|scope| {
        let responder = scope.spawn(|| {
            let mut primary = None;
            let mut observed = None;
            let mut observation_done = false;
            let mut joined = None;
            let mut closed_at = None;
            let end = Instant::now() + Duration::from_secs(20);
            while !done.load(Ordering::SeqCst) && Instant::now() < end {
                member.step_live(mode);
                if primary.is_none()
                    && root
                        .join(if active {
                            "session-generation-0.json"
                        } else if checking {
                            "quiesce-check-started"
                        } else {
                            "quiesce-leave"
                        })
                        .exists()
                {
                    primary = Some(management(
                        socket,
                        LifecycleDirective::Leave {
                            cause: Cause { uid: 74 },
                            forced: false,
                            rollback: false,
                        },
                    ));
                }
                if root.join("quiesce-all-closed").exists() && closed_at.is_none() {
                    closed_at = Some(Instant::now());
                }
                if root.join("quiesce-request").exists()
                    && (!mode.ends_with("listener")
                        || closed_at.is_some_and(|at| at.elapsed() >= Duration::from_millis(100)))
                {
                    if invalid || dead {
                        std::fs::write(root.join("quiesce-ack-release"), b"go").unwrap();
                    } else if observed.is_none() {
                        observed = Some(management(socket, LifecycleDirective::Observe));
                    }
                }
                if let Some(observed) = &observed
                    && !observation_done
                    && let Some(bytes) = read_packet(observed)
                {
                    let answer: s::enter::ObservationEnvelope =
                        serde_json::from_slice(&bytes).unwrap();
                    assert!(matches!(
                        answer.payload,
                        s::enter::ObservationContent::Answer(
                            s::enter::ObservationAnswer::InTransition { .. }
                        )
                    ));
                    observation_done = true;
                    std::fs::write(root.join("quiesce-observed"), b"yes").unwrap();
                    std::fs::write(root.join("quiesce-ack-release"), b"go").unwrap();
                }
                if checking && root.join("quiesce-close-0").exists() {
                    std::fs::write(root.join("quiesce-check-release"), b"go").unwrap();
                }
                if mode.ends_with("join") && observation_done && joined.is_none() {
                    joined = Some(management(
                        socket,
                        LifecycleDirective::JoinLeave {
                            cause: Cause { uid: 81 },
                        },
                    ));
                }
                std::thread::sleep(Duration::from_millis(1));
            }
            if !invalid && !dead {
                assert!(observation_done, "Quiesce did not service Observe");
            }
            if let Some(joined) = joined {
                assert!(read_packet(&joined).is_none());
            }
            primary.expect("actual Leave")
        });
        let recorder = scope.spawn(|| {
            let mut bytes = Vec::new();
            while !done.load(Ordering::SeqCst) {
                let _ = reader.read_to_end(&mut bytes);
                std::thread::sleep(Duration::from_millis(1));
            }
            let _ = reader.read_to_end(&mut bytes);
            bytes
        });
        let _stop = StopThreads(&done);
        let session = guard.0.as_mut().unwrap();
        assert!(!session.begin_graceful_quiesce(), "no Leave authority");
        if !checking {
            if !empty {
                drive(session, Progress::Accepted);
            }
            if active {
                session.start_turn(TurnKey("session-0".into())).unwrap();
            } else {
                std::fs::write(root.join("quiesce-leave"), b"go").unwrap();
            }
        }
        drive(session, Progress::DirectivePending);
        assert!(session.defer_graceful_leave());
        if active {
            assert!(
                !session.begin_graceful_quiesce(),
                "turn still owns generation and world close"
            );
            std::fs::write(root.join("graceful-resume-generation"), b"go").unwrap();
            drive(session, Progress::LeavePending);
            wait_file(root, "quiesce-turn-close");
        }
        assert!(session.begin_graceful_quiesce());
        assert!(!session.begin_graceful_quiesce(), "one Quiesce election");
        assert!(session.start_turn(TurnKey("forbidden".into())).is_err());
        assert!(session.next_input().is_err());
        let target = if invalid {
            Progress::Stopped
        } else if mode.ends_with("expiry") || dead {
            Progress::LeavePending
        } else if mode.ends_with("join") {
            Progress::DirectivePending
        } else {
            Progress::Quiesced
        };
        drive(session, target);
        if mode.ends_with("join") {
            assert!(session.join_graceful_leave());
            assert_eq!(session.progress(), Progress::LeavePending);
        }
        assert!(
            !session.begin_graceful_quiesce(),
            "completion/force cannot restart Quiesce"
        );
        if !invalid && !mode.ends_with("join") && !dead {
            wait_file(root, "quiesce-all-closed");
        }
        done.store(true, Ordering::SeqCst);
        (responder.join().unwrap(), recorder.join().unwrap())
    });
    assert!(read_packet(&primary).is_none(), "Quiesce is not Left");
    let input = guard.0.take().unwrap().into_owner().ok().unwrap();
    let retained = Guard {
        input: Some(input),
        management: None,
    };
    let input = retained.owned_input();
    if !empty && !dead && !mode.ends_with("join") {
        assert!(
            input.failed_world_was_sent(),
            "failed cleanup must retain the delivered input's exact send proof"
        );
    }
    let leave = input.serving.graceful.as_ref().unwrap();
    assert_eq!(leave.cause.uid, 74);
    assert_eq!(leave.forced, forced);
    assert_eq!(
        leave.forced_by.map(|cause| cause.uid),
        mode.ends_with("join").then_some(81)
    );
    assert_eq!(input.serving.owner.context.failure().is_some(), invalid);
    assert!(
        input
            .serving
            .owner
            .context
            .deadlines
            .get(crate::flow::pump::PumpDeadline::Leave)
            .is_none()
    );
    if !invalid && !mode.ends_with("join") && !dead {
        assert!(
            input.serving.owner.context.input.snapshot().is_none(),
            "all input checking/measurement custody settled"
        );
        assert!(
            input
                .serving
                .owner
                .context
                .deadlines
                .get(crate::flow::pump::PumpDeadline::GracefulDrain)
                .is_none()
        );
        assert!(
            input
                .token
                .as_ref()
                .is_none_or(|token| token.test_revoked())
        );
    }
    let rows = trace_through(
        reader,
        bytes,
        retained.recorded_sequence(),
        Instant::now() + Duration::from_secs(5),
    )
    .unwrap();
    assert_eq!(
        rows.iter()
            .filter(|row| row["kind"] == "turn.started")
            .count(),
        usize::from(active)
    );
    assert_eq!(
        rows.iter()
            .filter(|row| row["kind"] == "turn.closed")
            .count(),
        usize::from(active)
    );
    assert_eq!(
        rows.iter()
            .filter(|row| row["kind"] == "input.offered")
            .count(),
        usize::from(!empty),
        "queued/seed input never entered intake"
    );
    if !invalid && !mode.ends_with("join") && !dead {
        let refusals = rows
            .iter()
            .filter(|row| {
                row["kind"] == "refusal"
                    && matches!(
                        serde_json::from_value::<RefusalRecord>(row["payload"].clone()),
                        Ok(RefusalRecord::Lifecycle {
                            asked: LifecycleAsk::Leave,
                            refusal: LifecycleRefusal::Unloading
                        })
                    )
            })
            .count();
        assert_eq!(refusals, 3 - usize::from(active));
    }
}
