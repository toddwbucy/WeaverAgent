//! Real worker Session, lifecycle peers and gate sockets with declared model,
//! classifier and filing doubles. This is no production language certificate.
use super::*;
use weaver_types::Cause;

pub(in crate::flow::context) fn is_mode(mode: &str) -> bool {
    mode.contains("-session-graceful-wind-down-")
}
pub(in crate::flow::context) fn gate(mode: &str, root: &Path, fd: i32) {
    let mut bytes = vec![0; weaver_types::MAX_ENVELOPE_BYTES];
    if mode.ends_with("active") {
        packet!(fd, &frame(0, false));
        let n = recv(fd, &mut bytes, MsgFlags::empty()).unwrap();
        let reply: OrganEnvelope = serde_json::from_slice(&bytes[..n]).unwrap();
        assert_eq!(reply.exchange, frame(0, false).exchange);
        let Payload::Frame(answer) = reply.payload else {
            panic!("actual turn close before Quiesce");
        };
        let answer: serde_json::Value = serde_json::from_slice(&answer.octets().unwrap()).unwrap();
        assert_eq!(answer["kind"], "answered");
        std::fs::write(root.join("wind-prior-close"), &bytes[..n]).unwrap();
    }
    let n = recv(fd, &mut bytes, MsgFlags::empty()).unwrap();
    let quiesce: OrganEnvelope = serde_json::from_slice(&bytes[..n]).unwrap();
    assert!(matches!(
        quiesce.payload,
        Payload::Directive(LifecycleDirective::Quiesce)
    ));
    packet!(
        fd,
        &OrganEnvelope {
            exchange: quiesce.exchange,
            position: Position::Close,
            payload: Payload::Answer(LifecycleAnswer::GateQuiesced)
        }
    );
    std::fs::write(root.join("wind-quiesced"), b"yes").unwrap();
    if mode.ends_with("gate-dead") {
        wait_file(root, "wind-generate-0");
        return;
    }
    assert_eq!(
        recv(fd, &mut bytes, MsgFlags::empty()).unwrap_or(0),
        0,
        "wind-down must send no world reply, dispatch or Lower"
    );
}
pub(in crate::flow::context) fn spu(mode: &str, fd: i32) {
    let root = std::path::PathBuf::from(std::env::var("WEAVER_WORKER_INPUT_ROOT").unwrap());
    let mut bytes = vec![0; weaver_types::MAX_ENVELOPE_BYTES];
    let mut resident = 3;
    let mut measured = None;
    let mut generations = 0;
    let mut prior = mode.ends_with("active");
    loop {
        let n = recv(fd, &mut bytes, MsgFlags::empty()).unwrap_or(0);
        if n == 0 {
            return;
        }
        let request: TokenDirective = serde_json::from_slice(&bytes[..n]).unwrap();
        match request {
            TokenDirective::MeasureDelta {
                keep,
                delta,
                tool_schemas,
            } => {
                assert!(measured.is_none());
                assert_eq!(keep, resident);
                assert!(tool_schemas.is_empty());
                let correction = (mode.ends_with("correction")
                    || mode.ends_with("exhausted")
                    || mode.ends_with("correction-refused"))
                    && generations > 0;
                assert_eq!(
                    delta,
                    vec![Message {
                        role: Role::User,
                        content: vec![ContentBlock::Text {
                            text: if prior {
                                "Read caf\u{e9}."
                            } else if correction {
                                "Read correction."
                            } else {
                                "Read summary."
                            }
                            .into()
                        }]
                    }]
                );
                if !prior {
                    std::fs::write(
                        root.join(format!("wind-measure-{generations}")),
                        &bytes[..n],
                    )
                    .unwrap();
                    if generations == 0 {
                        wait_file(&root, "wind-measure-release");
                    }
                }
                measured = Some(delta);
                packet!(
                    fd,
                    &TokenAnswer::DeltaMeasured {
                        resident,
                        prefix_tokens: 3,
                        keep,
                        input_tokens: if mode.ends_with("placement-refused") {
                            16_383
                        } else {
                            3
                        },
                        capacity: 16_384
                    }
                );
            }
            TokenDirective::AppendAndGenerate {
                turn,
                delta,
                tool_schemas,
            } => {
                assert_eq!(turn.0, if prior { "session-0" } else { "wind-down-0" });
                assert_eq!(delta, measured.take().unwrap());
                assert!(tool_schemas.is_empty());
                if prior {
                    std::fs::write(root.join("wind-leave"), b"go").unwrap();
                    wait_file(&root, "wind-prior-release");
                } else {
                    std::fs::write(
                        root.join(format!("wind-generate-{generations}")),
                        &bytes[..n],
                    )
                    .unwrap();
                }
                if mode.contains("wind-down-join")
                    || mode.ends_with("gate-dead")
                    || (mode.ends_with("expiry") && generations == 1)
                {
                    continue;
                }
                if mode.ends_with("expiry") && generations == 0 {
                    std::thread::sleep(Duration::from_millis(550));
                }
                if mode.ends_with("decode-refused")
                    || (mode.ends_with("correction-refused") && generations > 0)
                {
                    packet!(fd, &weaver_types::TokenRefusal::MalformedDelta);
                    generations += 1;
                    continue;
                }
                resident += 5;
                let mut content = vec![ContentBlock::Text {
                    text: if mode.ends_with("exhausted")
                        || ((mode.ends_with("correction") || mode.ends_with("correction-refused"))
                            && generations == 0)
                    {
                        "Read bad."
                    } else {
                        "Read result."
                    }
                    .into(),
                }];
                if mode.ends_with("exhausted") && generations == 0 {
                    content.insert(
                        0,
                        ContentBlock::Text {
                            text: "Read file.".into(),
                        },
                    );
                    content.insert(
                        1,
                        ContentBlock::ToolCall(weaver_traits::ToolCall {
                            name: "sh".into(),
                            arguments: "retained request before rejected block".into(),
                        }),
                    );
                }
                if mode.ends_with("calls") {
                    content.push(ContentBlock::ToolCall(weaver_traits::ToolCall {
                        name: "sh".into(),
                        arguments: "opaque wind-down command".into(),
                    }));
                }
                let generation = Generation {
                    emission: "Read result.".into(),
                    finish: Finish::Completed,
                    content,
                    unrecovered: vec![],
                    request: serde_json::value::RawValue::from_string(format!(
                        "{{\"wind_down\":{generations}}}"
                    ))
                    .unwrap(),
                    measurement: serde_json::value::RawValue::from_string(
                        "{\"input_tokens\":[17,18,19],\"output_tokens\":[31]}".into(),
                    )
                    .unwrap(),
                    resident,
                    capacity: 16_384,
                };
                if prior {
                    prior = false;
                } else {
                    generations += 1;
                }
                packet!(fd, &TokenAnswer::Generated(generation));
                if mode.contains("-session-graceful-save") {
                    return;
                }
            }
            other => panic!("unexpected wind-down decode: {other:?}"),
        }
    }
}

pub(super) fn exercise(
    serving: FoundationServing<'_>,
    member: &mut Member,
    reader: &mut UnixStream,
    socket: &Path,
    root: &Path,
    mode: &str,
) {
    if mode.ends_with("author-wait") {
        return author_wait(serving, member, reader, socket, root, mode);
    }
    let active = mode.ends_with("active");
    let dead = mode.ends_with("gate-dead");
    let forced = mode.ends_with("expiry") || mode.contains("wind-down-join") || dead;
    let component = mode.ends_with("component");
    let no_decode =
        mode.ends_with("template-absent") || mode.ends_with("template-refused") || component;
    let refused = no_decode
        || mode.ends_with("placement-refused")
        || mode.ends_with("decode-refused")
        || mode.ends_with("exhausted")
        || mode.ends_with("correction-refused");
    let passes = if mode.ends_with("multiple") || mode.ends_with("expiry") {
        2
    } else {
        1
    };
    let held = mode.ends_with("held-filing");
    let mut guard = SessionGuard(Some(Session::begin(serving, &FAMILY, CutMethod::Flush)));
    let done = AtomicBool::new(false);
    reader.set_nonblocking(true).unwrap();
    let (primary, bytes) = std::thread::scope(|scope| {
        let responder = scope.spawn(|| {
            let mut primary = None;
            let mut observed = None;
            let mut observed_done = false;
            let mut joined = None;
            let mut held_at = None;
            if held {
                member.hold_kind = Some("loop.instruction");
            }
            let end = Instant::now() + Duration::from_secs(18);
            while !done.load(Ordering::SeqCst) && Instant::now() < end {
                member.step_live(mode);
                if primary.is_none() && root.join("wind-leave").exists() {
                    primary = Some(management(
                        socket,
                        LifecycleDirective::Leave {
                            cause: Cause { uid: 74 },
                            forced: false,
                            rollback: false,
                        },
                    ));
                }
                if member.held_file.is_some() && held_at.is_none() {
                    held_at = Some(Instant::now());
                }
                if observed.is_none()
                    && (root.join("wind-measure-0").exists() || (held && held_at.is_some()))
                {
                    observed = Some(management(socket, LifecycleDirective::Observe));
                }
                if let Some(observed) = &observed
                    && !observed_done
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
                    observed_done = true;
                    std::fs::write(root.join("wind-observed"), b"yes").unwrap();
                    std::fs::write(root.join("wind-measure-release"), b"yes").unwrap();
                }
                if held
                    && held_at.is_some_and(|at| at.elapsed() >= Duration::from_millis(100))
                    && observed_done
                    && member.hold_kind.is_some()
                {
                    assert!(
                        !root.join("wind-measure-0").exists(),
                        "measurement preceded the instruction's Filed receipt"
                    );
                    member.hold_kind = None;
                }
                if mode.contains("wind-down-join")
                    && joined.is_none()
                    && root.join("wind-generate-0").exists()
                {
                    joined = Some(management(
                        socket,
                        LifecycleDirective::JoinLeave {
                            cause: Cause { uid: 81 },
                        },
                    ));
                }
                std::thread::sleep(Duration::from_millis(1));
            }
            if !no_decode {
                assert!(observed_done, "wind-down did not service Observe");
            }
            if let Some(joined) = joined {
                assert!(
                    read_packet(&joined).is_none(),
                    "join is retained until the Leave tail"
                );
            }
            primary.unwrap()
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
        assert!(
            session
                .begin_graceful_wind_down(TurnKey("wind-down-0".into()), passes)
                .is_err()
        );
        if active {
            drive(session, Progress::Accepted);
            session.start_turn(TurnKey("session-0".into())).unwrap();
        } else {
            std::fs::write(root.join("wind-leave"), b"go").unwrap();
        }
        drive(session, Progress::DirectivePending);
        assert!(session.defer_graceful_leave());
        assert!(
            session
                .begin_graceful_wind_down(TurnKey("wind-down-0".into()), passes)
                .is_err(),
            "Quiesce is still owed"
        );
        if active {
            std::fs::write(root.join("wind-prior-release"), b"go").unwrap();
            drive(session, Progress::LeavePending);
            wait_file(root, "wind-prior-close");
        }
        assert!(session.begin_graceful_quiesce());
        drive(session, Progress::Quiesced);
        assert!(
            session
                .begin_graceful_wind_down(TurnKey("wind-down-0".into()), 0)
                .is_err()
        );
        session
            .begin_graceful_wind_down(TurnKey("wind-down-0".into()), passes)
            .unwrap();
        assert!(
            session
                .begin_graceful_wind_down(TurnKey("wind-down-1".into()), passes)
                .is_err()
        );
        assert!(session.next_input().is_err());
        assert!(session.start_turn(TurnKey("external".into())).is_err());
        drive(
            session,
            if component {
                Progress::Stopped
            } else if mode.ends_with("expiry") || dead {
                Progress::LeavePending
            } else if mode.contains("wind-down-join") {
                Progress::DirectivePending
            } else {
                Progress::WindDownComplete
            },
        );
        if mode.contains("wind-down-join") {
            if mode.ends_with("join-delayed") {
                std::thread::sleep(Duration::from_millis(1100));
            }
            assert!(session.join_graceful_leave());
            assert_eq!(session.progress(), Progress::LeavePending);
        }
        done.store(true, Ordering::SeqCst);
        (responder.join().unwrap(), recorder.join().unwrap())
    });
    assert!(read_packet(&primary).is_none(), "wind-down is not Left");
    let input = guard.0.take().unwrap().into_owner().ok().unwrap();
    let retained = Guard {
        input: Some(input),
        management: None,
    };
    let input = retained.owned_input();
    if active {
        assert!(
            input.failed_world_was_sent(),
            "wind-down retains the prior turn's real world-send proof"
        );
    }
    let leave = input.serving.graceful.as_ref().unwrap();
    assert_eq!(leave.cause.uid, 74);
    assert_eq!(leave.forced, forced);
    assert_eq!(
        leave.forced_by.map(|cause| cause.uid),
        mode.contains("wind-down-join").then_some(81)
    );
    assert_eq!(input.serving.owner.context.failure().is_some(), component);
    assert!(
        input
            .serving
            .owner
            .context
            .deadlines
            .get(PumpDeadline::Leave)
            .is_none(),
        "no early 150-second tail"
    );
    assert_eq!(
        input
            .serving
            .owner
            .context
            .deadlines
            .get(PumpDeadline::WindDown)
            .is_some(),
        component,
        "completed/converted phases retire their cap; failed custody retains it"
    );
    if mode.ends_with("expiry") {
        let work = leave.wind_down.as_ref().unwrap();
        assert_eq!(work.bound.unwrap().began_ns(), work.began_ns);
        assert_eq!(
            work.bound.unwrap().deadline_ns() - work.began_ns,
            1_000_000_000
        );
        assert!(
            root.join("wind-generate-1").exists(),
            "expiry must occur in a later pass"
        );
        assert!(
            input.serving.owner.context.generation.is_some(),
            "conversion retains the in-flight decoder"
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
            .filter(|row| row["kind"] == "turn.started" && row["turn"] == "wind-down-0")
            .count(),
        1
    );
    let closes: Vec<_> = rows
        .iter()
        .filter(|row| row["kind"] == "turn.closed" && row["turn"] == "wind-down-0")
        .collect();
    assert_eq!(closes.len(), usize::from(!forced && !component));
    assert!(!rows.iter().any(|row| (row["kind"] == "message.user"
        || row["kind"] == "input.offered")
        && (!active || row["turn"] == "wind-down-0")));
    let instructions: Vec<_> = rows
        .iter()
        .filter(|row| {
            row["kind"] == "loop.instruction" && row["payload"]["template_id"] == "wind_down"
        })
        .collect();
    assert_eq!(
        instructions.len(),
        if mode.ends_with("template-absent") {
            0
        } else {
            passes
        }
    );
    for instruction in &instructions {
        assert_eq!(
            instruction["payload"]["message"],
            serde_json::json!({"role":"user", "content":[{"type":"text","text":"Read summary."}]})
        );
        assert_eq!(
            instruction["payload"]["instruction"],
            serde_json::json!({"wind_down":{}})
        );
    }
    if !forced && !component {
        let close = closes[0];
        if refused {
            assert_eq!(close["payload"]["reason"], "refused", "{close}");
        } else if mode.ends_with("calls") {
            assert_eq!(close["payload"]["reason"], "unload", "{close}");
        } else {
            assert_eq!(
                close["payload"],
                serde_json::json!({"close":"clean"}),
                "{close}"
            );
        }
        assert!(input.serving.owner.context.open_turn.is_none());
        assert!(input.serving.owner.context.wind_down.is_none());
    }
    if mode.ends_with("calls") || mode.ends_with("exhausted") {
        let starts: Vec<_> = rows
            .iter()
            .filter(|row| row["kind"] == "tool.call.started")
            .collect();
        let ends: Vec<_> = rows
            .iter()
            .filter(|row| row["kind"] == "tool.call.completed")
            .collect();
        assert_eq!(starts.len(), 1);
        assert_eq!(ends.len(), 1);
        assert_eq!(
            starts[0]["payload"]["call_id"],
            ends[0]["payload"]["call_id"]
        );
        assert!(ends[0].to_string().contains("not_dispatched"));
        assert!(ends[0].to_string().contains(if mode.ends_with("calls") {
            "unload"
        } else {
            "repair_exhausted"
        }));
        assert!(
            super::super::emission::tests::sequence(ends[0])
                < super::super::emission::tests::sequence(closes[0])
        );
    }
    if mode.ends_with("template-refused") {
        assert!(rows.iter().any(
            |row| row["kind"] == "fault" && row.to_string().contains("state_template_rejected")
        ));
    }
    if mode.ends_with("template-absent") {
        assert!(rows.iter().any(|row| row["kind"] == "refusal" && row.to_string().contains("invalid_template")));
    }
    let generated = root
        .read_dir()
        .unwrap()
        .filter_map(Result::ok)
        .filter(|e| {
            e.file_name()
                .to_string_lossy()
                .starts_with("wind-generate-")
        })
        .count();
    if no_decode || mode.ends_with("placement-refused") {
        assert_eq!(generated, 0);
    } else if mode.ends_with("correction") || mode.ends_with("correction-refused") {
        assert_eq!(generated, 2);
    } else if mode.ends_with("exhausted") {
        assert_eq!(generated, 3);
    } else {
        assert_eq!(generated, passes);
    }
}

struct ObserveDuringWait<'a> {
    socket: &'a Path,
    peer: Option<std::os::fd::OwnedFd>,
}
impl crate::flow::clock::PumpIo for ObserveDuringWait<'_> {
    fn now(&mut self) -> std::io::Result<u64> {
        MonotonicIo.now()
    }
    fn wait(
        &mut self,
        interests: &[crate::flow::clock::WaitInterest<'_>],
        deadline: Option<u64>,
    ) -> std::io::Result<crate::flow::clock::WaitOutcome> {
        // The outer readiness observation has an immediate deadline. Send
        // only inside the blocking author wait, after its ticket is retained.
        if self.peer.is_none() && deadline.is_none_or(|end| end > self.now().unwrap()) {
            self.peer = Some(management(self.socket, LifecycleDirective::Observe));
        }
        MonotonicIo.wait(interests, deadline)
    }
}

fn author_wait(
    serving: FoundationServing<'_>,
    member: &mut Member,
    reader: &mut UnixStream,
    socket: &Path,
    root: &Path,
    mode: &str,
) {
    let mut session = SessionGuard(Some(Session::begin(serving, &FAMILY, CutMethod::Flush)));
    let primary = management(
        socket,
        LifecycleDirective::Leave {
            cause: Cause { uid: 74 },
            forced: false,
            rollback: false,
        },
    );
    let done = AtomicBool::new(false);
    let pause = AtomicBool::new(false);
    let paused = AtomicBool::new(false);
    reader.set_nonblocking(true).unwrap();
    let (retained, bytes) = std::thread::scope(|scope| {
        let responder = scope.spawn(|| {
            while !done.load(Ordering::SeqCst) {
                member.step_live(mode);
                std::thread::sleep(Duration::from_millis(1));
            }
        });
        let recorder = scope.spawn(|| {
            let mut bytes = Vec::new();
            while !done.load(Ordering::SeqCst) {
                if pause.load(Ordering::SeqCst) {
                    paused.store(true, Ordering::SeqCst);
                } else {
                    let _ = reader.read_to_end(&mut bytes);
                }
                std::thread::sleep(Duration::from_millis(1));
            }
            let _ = reader.read_to_end(&mut bytes);
            bytes
        });
        let _stop = StopThreads(&done);
        let worker = session.0.as_mut().unwrap();
        drive(worker, Progress::DirectivePending);
        assert!(worker.defer_graceful_leave());
        assert!(worker.begin_graceful_quiesce());
        drive(worker, Progress::Quiesced);
        worker
            .begin_graceful_wind_down(TurnKey("wind-down-0".into()), 1)
            .unwrap();
        // Keep the original Session's input and its Leave; no replacement
        // serving owner is constructed for this adversarial author wait.
        let mut retained = Guard {
            input: Some(session.0.take().unwrap().into_owner().ok().unwrap()),
            management: None,
        };
        pause.store(true, Ordering::SeqCst);
        let until = Instant::now() + Duration::from_secs(3);
        while !paused.load(Ordering::SeqCst) {
            assert!(Instant::now() < until, "real trace reader did not pause");
            std::thread::sleep(Duration::from_millis(1));
        }
        let input = retained.input.as_mut().unwrap();
        let context = &mut input.serving.owner.context;
        let bound = context.deadlines.get(PumpDeadline::WindDown).unwrap();
        let mut clock = FakePumpIo::new(context.last_ns);
        assert!(super::super::super::management::stop::tests::fill_trace(context, &mut clock) > 0);
        let mut io = ObserveDuringWait { socket, peer: None };
        assert_eq!(
            input.advance_with_io(&mut io),
            FoundationInputProgress::ManagementPending,
            "Observe must interrupt the blocked author before its phase bound"
        );
        let peer = io.peer.unwrap();
        let pending = input.serving.owner.context.pending_author().unwrap();
        assert_eq!(pending.event.kind, serving::Kind::TurnStarted);
        assert_eq!(pending.event.turn, Some(TurnRef("wind-down-0".into())));
        let ticket = pending.ticket;
        let original =
            serde_json::to_value(input.serving.owner.context.barrier.pending().unwrap()).unwrap();
        retained.management = Some(retained.input.take().unwrap().management().ok().unwrap());
        let until = Instant::now() + Duration::from_secs(2);
        while retained.management.as_ref().unwrap().progress()
            != super::super::super::management::FoundationManagementProgress::Answered
        {
            assert!(
                Instant::now() < until,
                "Observe did not answer during author pressure"
            );
            retained.management.as_mut().unwrap().advance();
        }
        let answer: s::enter::ObservationEnvelope =
            serde_json::from_slice(&read_packet(&peer).unwrap()).unwrap();
        assert!(matches!(
            answer.payload,
            s::enter::ObservationContent::Answer(s::enter::ObservationAnswer::InTransition { .. })
        ));
        retained.input = Some(retained.management.take().unwrap().finish().ok().unwrap());
        let input = retained.input.as_mut().unwrap();
        let context = &input.serving.owner.context;
        assert_eq!(context.pending_author().unwrap().ticket, ticket);
        assert_eq!(
            serde_json::to_value(context.barrier.pending().unwrap()).unwrap(),
            original
        );
        assert_eq!(context.deadlines.get(PumpDeadline::WindDown), Some(bound));
        assert!(!root.join("wind-measure-0").exists());
        assert!(read_packet(&primary).is_none());
        pause.store(false, Ordering::SeqCst);
        std::fs::write(root.join("wind-measure-release"), b"Observe answered").unwrap();
        let until = Instant::now() + Duration::from_secs(4);
        use weaver_harness::FoundationWindDownProgress as W;
        loop {
            assert!(Instant::now() < until, "retained wind-down did not finish");
            assert_eq!(
                input.progress(),
                FoundationInputProgress::WindDown,
                "{:?}",
                input.detail()
            );
            match input.wind_down_progress().unwrap() {
                W::Pending => {
                    input.advance();
                }
                W::Placed => assert!(input.generate_wind_down()),
                W::Generated => assert!(input.repair_wind_down(&FAMILY)),
                W::Effective => assert!(input.finish_wind_down_pass()),
                W::PassComplete => assert!(input.close_wind_down()),
                W::Complete => break,
                other => panic!("clean retained author reached {other:?}"),
            }
        }
        assert!(input.serving.owner.context.failure().is_none());
        assert!(
            input
                .serving
                .owner
                .context
                .deadlines
                .get(PumpDeadline::WindDown)
                .is_none()
        );
        done.store(true, Ordering::SeqCst);
        responder.join().unwrap();
        (retained, recorder.join().unwrap())
    });
    let rows = trace_through(
        reader,
        bytes,
        retained.recorded_sequence(),
        Instant::now() + Duration::from_secs(5),
    )
    .unwrap();
    for kind in ["turn.started", "turn.closed"] {
        assert_eq!(
            rows.iter()
                .filter(|row| row["kind"] == kind && row["turn"] == "wind-down-0")
                .count(),
            1
        );
    }
    assert!(!rows.iter().any(|row| row["kind"] == "message.user"));
    assert!(read_packet(&primary).is_none(), "wind-down is not Left");
}
