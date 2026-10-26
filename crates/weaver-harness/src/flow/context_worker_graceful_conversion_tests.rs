//! Force the original graceful Leave while its real drain still owns input.
//! The normal SQLite member withholds a committed filing until actual Lower.
//! Every case continues through capture and Left; no phase receipt is invented.
use super::*;

pub(super) struct Evidence {
    waits: Vec<(PumpDeadline, crate::flow::clock::AbsoluteBound)>,
    turn: Option<weaver_trace::TurnRef>,
    generation: bool,
}
impl Evidence {
    pub(super) fn assert_run(&self, run: &FoundationRun) {
        assert_eq!(
            run.context
                .deadlines
                .entries()
                .filter(|(key, _)| *key != PumpDeadline::Leave)
                .collect::<Vec<_>>(),
            self.waits
        );
        assert_eq!(run.context.open_turn, self.turn);
        assert_eq!(run.context.generation.is_some(), self.generation);
        assert!(run.context.failure().is_none());
        assert!(
            run.context
                .deadlines
                .get(PumpDeadline::GracefulDrain)
                .is_none()
        );
        assert!(run.context.deadlines.get(PumpDeadline::WindDown).is_none());
    }
}

pub(super) fn is_mode(mode: &str) -> bool {
    mode.contains("-session-graceful-save-sqlite-drain-")
}

pub(super) fn handoff(
    serving: FoundationServing<'_>,
    socket: &Path,
    root: &Path,
    mode: &str,
    member: &mut Member,
    reader: &mut UnixStream,
) -> Handoff {
    let held = mode.ends_with("-held-filing");
    let generating = mode.ends_with("-generation");
    let started = held || generating;
    let expiry = mode.contains("-drain-expiry");
    let mut session = SessionGuard(Some(Session::begin(serving, &FAMILY, CutMethod::Flush)));
    let done = AtomicBool::new(false);
    let request_leave = AtomicBool::new(false);
    reader.set_nonblocking(true).unwrap();
    let (actual, peer, before, joins, evidence) = std::thread::scope(|scope| {
        let responder = scope.spawn(|| {
            let end = Instant::now() + Duration::from_secs(15);
            let mut peer = None;
            while !done.load(Ordering::SeqCst) && Instant::now() < end {
                member.step_live(mode);
                if peer.is_none()
                    && (request_leave.load(Ordering::SeqCst)
                        || (held && member.normal_filing_held())
                        || (generating && root.join("session-generation-0.json").exists()))
                {
                    assert!(!root.join("ready-rollback-lower.json").exists());
                    peer = Some(management(
                        socket,
                        LifecycleDirective::Leave {
                            cause: weaver_types::Cause { uid: 74 },
                            forced: false,
                            rollback: false,
                        },
                    ));
                }
                std::thread::sleep(Duration::from_millis(1));
            }
            peer.expect("the actual original graceful request was sent")
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
        let flow = session.0.as_mut().unwrap();
        if !mode.ends_with("-idle") {
            drive(flow, Progress::Accepted);
        }
        if started {
            flow.start_turn(TurnKey("session-0".into())).unwrap();
        } else {
            request_leave.store(true, Ordering::SeqCst);
        }
        drive(flow, Progress::DirectivePending);
        assert!(flow.defer_graceful_leave());
        let mut guard = Guard {
            input: Some(session.0.take().unwrap().into_owner().ok().unwrap()),
            management: None,
        };
        let original = guard
            .input
            .as_ref()
            .unwrap()
            .serving
            .graceful
            .as_ref()
            .unwrap();
        assert!(!original.forced);
        let heard = original.heard_ns;
        let drain = original.drain;
        assert_eq!(drain.is_some(), expiry);
        // A pending but unconverted request still owes its graceful phases.
        guard.input = Some(match guard.input.take().unwrap().graceful_leave_handoff() {
            Err(input) => input,
            Ok(_) => panic!("unforced drain skipped Quiesce and wind-down"),
        });
        let engine = guard
            .input
            .as_ref()
            .unwrap()
            .engine
            .as_deref()
            .map(|engine| std::ptr::from_ref(engine) as usize);
        assert_eq!(engine.is_some(), started);
        let original_turn = guard
            .input
            .as_ref()
            .unwrap()
            .serving
            .owner
            .context
            .open_turn
            .clone();
        assert_eq!(original_turn.is_some(), started);
        if expiry {
            let end = Instant::now() + Duration::from_secs(3);
            while !guard
                .input
                .as_ref()
                .unwrap()
                .serving
                .graceful
                .as_ref()
                .unwrap()
                .forced
            {
                assert!(Instant::now() < end, "original drain did not expire");
                let input = guard.input.as_mut().unwrap();
                assert_ne!(
                    input.advance(),
                    FoundationInputProgress::Stopped,
                    "{:?}",
                    input.detail()
                );
            }
            let input = guard.input.as_ref().unwrap();
            // The pump can report the exact expired bound before its caller
            // copies the pump's latest clock into RecordingContext.last_ns.
            assert!(
                drain
                    .unwrap()
                    .expired(crate::flow::clock::monotonic_ns().unwrap())
            );
            assert!(input.serving.graceful.as_ref().unwrap().forced_by.is_none());
        }
        let mut joins = Vec::new();
        if mode.contains("-join-") {
            for (uid, ordinal) in [(75, 100), (76, 101)] {
                let peer = joining(socket, uid, ordinal);
                let end = Instant::now() + Duration::from_secs(3);
                while guard.input.as_ref().unwrap().progress()
                    != FoundationInputProgress::ManagementPending
                {
                    assert!(Instant::now() < end, "actual JoinLeave was not heard");
                    let input = guard.input.as_mut().unwrap();
                    assert_ne!(input.wait_for_wake(), FoundationInputProgress::Stopped);
                }
                guard.management = Some(guard.input.take().unwrap().management().ok().unwrap());
                while guard.management.as_ref().unwrap().progress()
                    == weaver_harness::FoundationManagementProgress::Pending
                {
                    assert!(Instant::now() < end, "actual JoinLeave was not parsed");
                    guard.management.as_mut().unwrap().advance();
                }
                let owner = guard.management.take().unwrap();
                guard.input = Some(match owner.join_graceful_leave() {
                    Ok(input) => input,
                    Err(owner) => {
                        guard.management = Some(owner);
                        panic!("actual JoinLeave was not retained");
                    }
                });
                assert!(read_packet(&peer).is_none());
                joins.push(JoinReply {
                    peer,
                    ordinal,
                    answer: None,
                });
            }
        }
        let input = guard.input.as_ref().unwrap();
        let original = input.serving.graceful.as_ref().unwrap();
        assert!(original.forced);
        assert_eq!(
            original.forced_by,
            (!expiry).then_some(weaver_types::Cause { uid: 75 })
        );
        assert_eq!(original.heard_ns, heard);
        assert_eq!(original.drain, drain);
        assert_eq!(original.joins.len(), joins.len());
        assert!(original.quiesce.is_none() && original.wind_down.is_none());
        assert_eq!(input.serving.owner.context.open_turn, original_turn);
        let decision = input
            .serving
            .owner
            .context
            .deadlines
            .get(PumpDeadline::Decision);
        // Successful append admission retires Decision. The filing wait keeps
        // it; preserve every applicable original wait in either case.
        assert_eq!(decision.is_some(), held);
        assert_eq!(input.serving.owner.context.generation.is_some(), generating);
        let waits = input.serving.owner.context.deadlines.entries().collect();
        assert!(
            input
                .serving
                .owner
                .context
                .deadlines
                .get(PumpDeadline::Leave)
                .is_none()
        );
        assert!(
            input
                .serving
                .owner
                .context
                .deadlines
                .get(PumpDeadline::GracefulDrain)
                .is_none()
        );
        assert!(!root.join("ready-rollback-lower.json").exists());
        let before_handoff = crate::flow::clock::monotonic_ns().unwrap();
        let actual = match guard.input.take().unwrap().graceful_leave_handoff() {
            Ok(actual) => actual,
            Err(input) => {
                let detail = input.detail();
                guard.input = Some(input);
                panic!(
                    "converted drain must carry the actual unfinished input into Lower: {detail:?}"
                );
            }
        };
        assert_eq!(actual.inner.input.test_engine_identity(), engine);
        assert_eq!(
            actual.inner.message.envelope.payload,
            Payload::Directive(LifecycleDirective::Leave {
                cause: weaver_types::Cause { uid: 74 },
                forced: false,
                rollback: false,
            })
        );
        assert!(actual.inner.leave.began_ns() >= before_handoff);
        assert_eq!(
            actual.inner.leave.deadline_ns() - actual.inner.leave.began_ns(),
            weaver_types::LEAVE_BOUND_MS * 1_000_000
        );
        // Inspect the original run after the serving loan ends, before cleanup.
        let evidence = Evidence {
            waits,
            turn: original_turn,
            generation: generating,
        };
        done.store(true, Ordering::SeqCst);
        (
            actual,
            responder.join().unwrap(),
            recorder.join().unwrap(),
            joins,
            evidence,
        )
    });
    if held {
        assert!(member.normal_filing_held());
        assert_eq!(member.filings.len(), 1);
    }
    assert!(read_packet(&peer).is_none());
    Handoff {
        actual: LeaveHandoff::Graceful(actual),
        peer: Some(peer),
        socket: socket.into(),
        before,
        joins,
        conversion: Some(evidence),
        quiesce: None,
        wind_down: None,
    }
}

pub(super) fn assert_tail(
    rows: &[serde_json::Value],
    joins: &[JoinReply],
    filings: &[s::StateFrame],
    root: &Path,
    mode: &str,
) {
    let held = mode.ends_with("-held-filing");
    let generating = mode.ends_with("-generation");
    let started_turn = held || generating;
    let unload = rows.iter().find(|r| r["kind"] == "unload").unwrap();
    assert_eq!(unload["payload"]["forced"], true);
    assert_eq!(
        unload["payload"]["forced_by"],
        if mode.contains("-drain-expiry") {
            serde_json::Value::Null
        } else {
            serde_json::json!({"uid":75})
        }
    );
    assert_eq!(joins.len(), if mode.contains("-join-") { 2 } else { 0 });
    assert!(joins.iter().all(|join| join.answer.is_some()));
    assert!(!rows.iter().any(|r| r["turn"] == "wind-down-0"
        || (r["kind"] == "loop.instruction" && r["payload"]["template_id"] == "wind_down")));
    let lower: OrganEnvelope =
        serde_json::from_slice(&std::fs::read(root.join("ready-rollback-lower.json")).unwrap())
            .unwrap();
    assert_eq!(
        lower.exchange.ordinal, 2,
        "converted drain goes directly from Raise to Lower"
    );
    let started: Vec<_> = rows
        .iter()
        .filter(|r| r["kind"] == "turn.started")
        .collect();
    let closed: Vec<_> = rows.iter().filter(|r| r["kind"] == "turn.closed").collect();
    assert_eq!(started.len(), usize::from(started_turn));
    assert_eq!(closed.len(), usize::from(started_turn));
    assert_eq!(filings.iter().filter(|frame| matches!(frame, s::StateFrame::File { filing, .. } if filing.source.kind == "message.user")).count(), usize::from(started_turn));
    if generating {
        let cancel: TokenDirective = serde_json::from_slice(
            &std::fs::read(root.join("graceful-drain-cancel.json")).unwrap(),
        )
        .unwrap();
        assert_eq!(
            cancel,
            TokenDirective::Cancel {
                turn: TurnKey("session-0".into())
            }
        );
        assert!(
            !root.join("session-generation-1.json").exists(),
            "conversion cannot retry the model"
        );
    }
    if started_turn {
        let close = closed[0];
        assert_eq!(close["turn"], "session-0");
        assert_eq!(close["payload"]["close"], "stopped");
        assert_eq!(close["payload"]["reason"], "unload");
        assert_eq!(close["payload"]["cause"]["uid"], 74);
        let sequence =
            |row: &serde_json::Value| row["sequence"].as_str().unwrap().parse::<u64>().unwrap();
        let confirmed = rows
            .iter()
            .find(|row| row["kind"] == "save_point" && row["payload"]["phase"] == "confirmed")
            .unwrap();
        assert!(sequence(started[0]) < sequence(close));
        assert!(sequence(close) < sequence(confirmed));
    }
}
