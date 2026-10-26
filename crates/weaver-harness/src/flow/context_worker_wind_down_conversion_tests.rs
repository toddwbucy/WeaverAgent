//! Real original wind-down owners, root joins, Lower and normal SQLite capture.
//! The classifier and model are declared doubles; no serving selection is changed.
use super::*;
use crate::flow::clock::{AbsoluteBound, PumpIo, WaitInterest, WaitOutcome};
use weaver_harness::FoundationWindDownProgress as W;

pub(in crate::flow::context::worker_startup) fn is_mode(mode: &str) -> bool {
    mode.contains("-session-graceful-save-sqlite-wind-convert-")
}
fn case(mode: &str) -> &str {
    mode.split("-wind-convert-").nth(1).unwrap()
}
fn correction(mode: &str) -> bool {
    case(mode).starts_with("correction-") || case(mode) == "restatement"
}

pub(super) struct Evidence {
    waits: Vec<(PumpDeadline, AbsoluteBound)>,
    turn: Option<TurnRef>,
    ticket: Option<u64>,
    event: Option<serde_json::Value>,
    completed_author: Option<weaver_trace::Sequence>,
    held_author: Option<(usize, String)>,
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
        assert_eq!(run.context.completed_author, self.completed_author);
        if let Some(ticket) = self.ticket {
            assert_eq!(run.context.pending_author().unwrap().ticket, ticket);
            assert_eq!(
                Some(serde_json::to_value(run.context.barrier.pending().unwrap()).unwrap()),
                self.event
            );
        }
        assert!(run.context.failure().is_none());
        assert!(run.context.deadlines.get(PumpDeadline::WindDown).is_none());
    }
}

pub(in crate::flow::context::worker_startup) fn gate(mode: &str, root: &Path, fd: i32) {
    let mut bytes = vec![0; weaver_types::MAX_ENVELOPE_BYTES];
    if case(mode) == "prior-generation" {
        packet!(fd, &frame(0, false));
        let n = recv(fd, &mut bytes, MsgFlags::empty()).unwrap();
        let reply: OrganEnvelope = serde_json::from_slice(&bytes[..n]).unwrap();
        assert_eq!(reply.exchange, frame(0, false).exchange);
        assert!(matches!(reply.payload, Payload::Frame(_)));
        std::fs::write(root.join("wc-prior-close"), &bytes[..n]).unwrap();
    }
    let n = recv(fd, &mut bytes, MsgFlags::empty()).unwrap();
    let quiesce: OrganEnvelope = serde_json::from_slice(&bytes[..n]).unwrap();
    assert_eq!(
        quiesce.payload,
        Payload::Directive(LifecycleDirective::Quiesce)
    );
    assert_eq!(quiesce.exchange.ordinal, 2);
    packet!(
        fd,
        &OrganEnvelope {
            exchange: quiesce.exchange,
            position: Position::Close,
            payload: Payload::Answer(LifecycleAnswer::GateQuiesced)
        }
    );
    let n = recv(fd, &mut bytes, MsgFlags::empty()).unwrap();
    let lower: OrganEnvelope = serde_json::from_slice(&bytes[..n]).unwrap();
    assert_eq!(
        lower.payload,
        Payload::Directive(LifecycleDirective::Lower),
        "wind-down has no world reply or dispatch"
    );
    assert_eq!(lower.exchange.ordinal, 3);
    std::fs::write(root.join("ready-rollback-lower.json"), &bytes[..n]).unwrap();
    packet!(
        fd,
        &OrganEnvelope {
            exchange: lower.exchange,
            position: Position::Close,
            payload: Payload::Answer(LifecycleAnswer::GateStopped)
        }
    );
}

pub(in crate::flow::context::worker_startup) fn spu(mode: &str, fd: i32) {
    let root = std::path::PathBuf::from(std::env::var("WEAVER_WORKER_INPUT_ROOT").unwrap());
    let mut bytes = vec![0; weaver_types::MAX_ENVELOPE_BYTES];
    let mut resident = 3;
    let mut measured = None;
    let mut generations = 0;
    let mut pending: Option<(TurnKey, Generation)> = None;
    loop {
        let n = recv(fd, &mut bytes, MsgFlags::empty()).unwrap_or(0);
        if n == 0 {
            return;
        }
        let directive: TokenDirective = serde_json::from_slice(&bytes[..n]).unwrap();
        match directive {
            TokenDirective::MeasureDelta {
                keep,
                delta,
                tool_schemas,
            } => {
                assert!(measured.is_none() && pending.is_none() && tool_schemas.is_empty());
                assert_eq!(keep, resident);
                measured = Some(delta);
                std::fs::write(root.join(format!("wc-measure-{generations}")), &bytes[..n])
                    .unwrap();
                if case(mode) == "measure" || (case(mode) == "correction-place" && generations > 0)
                {
                    wait_file(&root, "ready-rollback-lower.json");
                }
                packet!(
                    fd,
                    &TokenAnswer::DeltaMeasured {
                        resident,
                        prefix_tokens: 3,
                        keep,
                        input_tokens: 3,
                        capacity: 16_384
                    }
                );
            }
            TokenDirective::AppendAndGenerate {
                turn,
                delta,
                tool_schemas,
            } => {
                assert_eq!(delta, measured.take().unwrap());
                assert!(tool_schemas.is_empty() && pending.is_none());
                let prior = turn.0 == "session-0";
                if prior {
                    assert_eq!(case(mode), "prior-generation");
                } else {
                    assert_eq!(turn.0, "wind-down-0");
                }
                resident += 5;
                let mut content = vec![ContentBlock::Text {
                    text: if correction(mode) && generations == 0 {
                        "Read bad."
                    } else {
                        "Read result."
                    }
                    .into(),
                }];
                if case(mode) == "pair-filing" {
                    content.push(ContentBlock::ToolCall(weaver_traits::ToolCall {
                        name: "sh".into(),
                        arguments: "original wind-down request".into(),
                    }));
                }
                let generation = Generation {
                    emission: "Read result.".into(),
                    finish: Finish::Completed,
                    content,
                    unrecovered: vec![],
                    request: serde_json::value::RawValue::from_string(format!(
                        "{{\"wind_conversion\":{generations}}}"
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
                    packet!(fd, &TokenAnswer::Generated(generation));
                    continue;
                }
                std::fs::write(
                    root.join(format!("wc-generation-{generations}")),
                    &bytes[..n],
                )
                .unwrap();
                let hold = matches!(
                    case(mode),
                    "generation" | "expiry-generation" | "prior-generation"
                ) || (case(mode) == "correction-generation" && generations > 0);
                generations += 1;
                if hold {
                    pending = Some((turn, generation));
                } else {
                    packet!(fd, &TokenAnswer::Generated(generation));
                }
            }
            TokenDirective::Cancel { turn } => {
                let (original, mut generation) =
                    pending.take().expect("original wind-down generation");
                assert_eq!(turn, original);
                assert_eq!(turn.0, "wind-down-0");
                assert!(!root.join("wc-cancel.json").exists());
                std::fs::write(root.join("wc-cancel.json"), &bytes[..n]).unwrap();
                wait_file(&root, "ready-rollback-lower.json");
                generation.finish = Finish::Stopped;
                packet!(fd, &TokenAnswer::Generated(generation));
                packet!(fd, &TokenAnswer::AtRest);
            }
            other => panic!("unexpected wind-down conversion decode: {other:?}"),
        }
    }
}

struct JoinDuringWait<'a> {
    socket: &'a Path,
    peer: Option<OwnedFd>,
}

/// Pause a real clock reading at the original wind-down deadline. The caller
/// checks that this interrupts the actual Recorded close before its owner has
/// consumed the receipt. No time value or completion is manufactured.
struct CloseClock {
    reads: usize,
    cut: usize,
    deadline: u64,
}
impl PumpIo for CloseClock {
    fn now(&mut self) -> std::io::Result<u64> {
        self.reads += 1;
        let now = MonotonicIo.now()?;
        if self.reads == self.cut && now < self.deadline {
            std::thread::sleep(Duration::from_nanos(self.deadline - now));
        }
        MonotonicIo.now()
    }
    fn wait(
        &mut self,
        interests: &[WaitInterest<'_>],
        deadline: Option<u64>,
    ) -> std::io::Result<WaitOutcome> {
        MonotonicIo.wait(interests, deadline)
    }
}
impl PumpIo for JoinDuringWait<'_> {
    fn now(&mut self) -> std::io::Result<u64> {
        MonotonicIo.now()
    }
    fn wait(
        &mut self,
        interests: &[WaitInterest<'_>],
        deadline: Option<u64>,
    ) -> std::io::Result<WaitOutcome> {
        if self.peer.is_none() && deadline.is_none_or(|end| end > self.now().unwrap()) {
            self.peer = Some(joining(self.socket, 75, 100));
        }
        MonotonicIo.wait(interests, deadline)
    }
}

struct LocalInspectionReady;
impl PumpIo for LocalInspectionReady {
    fn now(&mut self) -> std::io::Result<u64> {
        MonotonicIo.now()
    }
    fn wait(
        &mut self,
        interests: &[WaitInterest<'_>],
        deadline: Option<u64>,
    ) -> std::io::Result<WaitOutcome> {
        let now = MonotonicIo.now()?;
        assert!(
            deadline.is_some_and(|end| end <= now),
            "local inspection must advance before blocking"
        );
        MonotonicIo.wait(interests, deadline)
    }
}

fn retain_held_turn(input: &mut FoundationInput<'_>, resume: bool) -> (usize, String) {
    let mut original = None;
    let mut decision = None;
    for _ in 0..3 {
        begin_original_inspection(input);
        // Receive the normal member's actual reply without reducing it. The
        // next author step must progress that admitted local work before sleep.
        let end = Instant::now() + Duration::from_secs(5);
        while input
            .serving
            .owner
            .context
            .flow
            .state()
            .held_measurement()
            .is_none()
        {
            assert!(
                Instant::now() < end,
                "normal inspection reply did not arrive"
            );
            input
                .serving
                .owner
                .context
                .pump_once(&mut PumpPorts {
                    lifecycle: LifecyclePorts::default(),
                    gate: None,
                    decode: None,
                    label: None,
                    io: &mut MonotonicIo,
                })
                .unwrap();
        }
        assert_eq!(
            input.advance_with_io(&mut LocalInspectionReady),
            FoundationInputProgress::WindDown,
            "{:?}",
            input.detail()
        );
        let context = &input.serving.owner.context;
        let work = input
            .serving
            .graceful
            .as_ref()
            .unwrap()
            .wind_down
            .as_ref()
            .unwrap();
        let held = work
            .test_unadmitted_author()
            .expect("the exact caller regained its unadmitted event");
        if let Some(original) = &original {
            assert_eq!(
                &held, original,
                "another attempt must move the original event"
            );
            assert_eq!(context.deadlines.get(PumpDeadline::Decision), decision);
        } else {
            original = Some(held);
            decision = context.deadlines.get(PumpDeadline::Decision);
            assert!(decision.is_some());
        }
        assert!(!context.caller_author_held());
        assert!(context.pending_author().is_none() && context.completed_author.is_none());
        assert!(
            context.inspection.is_some(),
            "the earlier real inspection still owns its answer"
        );
        assert!(context.failure().is_none());
        finish_original_inspection(input);
    }
    assert_eq!(
        input
            .serving
            .owner
            .context
            .deadlines
            .get(PumpDeadline::Decision),
        decision
    );
    if resume {
        assert_eq!(
            input.advance(),
            FoundationInputProgress::WindDown,
            "{:?}",
            input.detail()
        );
        assert!(
            input
                .serving
                .graceful
                .as_ref()
                .unwrap()
                .wind_down
                .as_ref()
                .unwrap()
                .test_unadmitted_author()
                .is_none()
        );
    }
    original.unwrap()
}

fn begin_original_inspection(input: &mut FoundationInput<'_>) {
    use crate::flow::context::{InspectionBeginRefusal, InspectionKind};
    let end = Instant::now() + Duration::from_secs(5);
    loop {
        let context = &mut input.serving.owner.context;
        match context.begin_inspection(InspectionKind::Shape, None, &mut MonotonicIo) {
            Ok(()) => return,
            Err(error) => {
                assert_eq!(error.refusal, InspectionBeginRefusal::Busy, "{error:?}");
                assert!(error.failure.is_none() && !error.retained, "{error:?}");
                assert!(
                    Instant::now() < end,
                    "preceding account never allowed inspection"
                );
            }
        }
        // A prior content filing can still own its actual member exchange.
        // Settle it through the common reducer and readiness wait before the
        // inspection asks to own that same channel.
        if context.reduce_flow(&mut MonotonicIo).unwrap()
            == crate::flow::coordinator::Advance::Pending
        {
            context
                .pump_once(&mut PumpPorts {
                    lifecycle: LifecyclePorts::default(),
                    gate: None,
                    decode: None,
                    label: None,
                    io: &mut MonotonicIo,
                })
                .unwrap();
        }
    }
}

fn finish_original_inspection(input: &mut FoundationInput<'_>) {
    use crate::flow::context::{InspectionCompletion, InspectionProgress};
    let end = Instant::now() + Duration::from_secs(5);
    loop {
        assert!(Instant::now() < end, "original inspection did not settle");
        let context = &mut input.serving.owner.context;
        let mut io = MonotonicIo;
        let mut ports = PumpPorts {
            lifecycle: LifecyclePorts::default(),
            gate: None,
            decode: None,
            label: None,
            io: &mut io,
        };
        let progress = context.advance_inspection(&mut ports).unwrap();
        if let Some(completion) = context.take_inspection(ports.io).unwrap() {
            assert!(matches!(completion, InspectionCompletion::Shape { .. }));
            return;
        }
        if progress == InspectionProgress::Pending {
            context.pump_once(&mut ports).unwrap();
        }
    }
}

pub(super) fn handoff(
    serving: FoundationServing<'_>,
    socket: &Path,
    root: &Path,
    mode: &str,
    member: &mut Member,
    reader: &mut UnixStream,
) -> Handoff {
    let scenario = case(mode);
    let expiry = matches!(scenario, "expiry-generation" | "expiry-close-recorded");
    let author = matches!(scenario, "start-author" | "close-author");
    let mut session = SessionGuard(Some(Session::begin(serving, &FAMILY, CutMethod::Flush)));
    let done = AtomicBool::new(false);
    let pause = AtomicBool::new(false);
    let paused = AtomicBool::new(false);
    reader.set_nonblocking(true).unwrap();
    let (actual, primary, before, joins, evidence) = std::thread::scope(|scope| {
        let responder = scope.spawn(|| {
            let mut peer = None;
            let mut pair_armed = false;
            while !done.load(Ordering::SeqCst) {
                if scenario == "pair-filing" && !pair_armed && root.join("wc-arm-pair").exists() {
                    member.hold_next_normal_filing();
                    pair_armed = true;
                    std::fs::write(root.join("wc-pair-armed"), b"actual proxy armed").unwrap();
                }
                member.step_live(mode);
                let trigger = match scenario {
                    "held-filing" | "pair-filing" => member.normal_filing_held(),
                    "measure" => root.join("wc-measure-0").exists(),
                    "correction-place" => root.join("wc-measure-1").exists(),
                    "generation" | "prior-generation" => root.join("wc-generation-0").exists(),
                    "correction-generation" => root.join("wc-generation-1").exists(),
                    _ => false,
                };
                if trigger && peer.is_none() {
                    peer = Some(joining(socket, 75, 100));
                }
                std::thread::sleep(Duration::from_millis(1));
            }
            peer
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
        if scenario == "prior-generation" {
            drive(worker, Progress::Accepted);
            worker.start_turn(TurnKey("session-0".into())).unwrap();
            drive(worker, Progress::Completed);
        }
        let primary = management(
            socket,
            LifecycleDirective::Leave {
                cause: weaver_types::Cause { uid: 74 },
                forced: false,
                rollback: false,
            },
        );
        let mut guard = Guard {
            input: Some(session.0.take().unwrap().into_owner().ok().unwrap()),
            management: None,
        };
        let prepare_end = Instant::now() + Duration::from_secs(5);
        while guard.input.as_ref().unwrap().progress() != FoundationInputProgress::ManagementPending
        {
            assert!(Instant::now() < prepare_end);
            assert_ne!(
                guard.input.as_mut().unwrap().advance(),
                FoundationInputProgress::Stopped
            );
            std::thread::sleep(Duration::from_millis(1));
        }
        guard.management = Some(guard.input.take().unwrap().management().ok().unwrap());
        while guard.management.as_ref().unwrap().progress()
            == weaver_harness::FoundationManagementProgress::Pending
        {
            assert!(Instant::now() < prepare_end);
            guard.management.as_mut().unwrap().advance();
        }
        guard.input = Some(
            match guard.management.take().unwrap().defer_graceful_leave() {
                Ok(input) => input,
                Err(owner) => {
                    guard.management = Some(owner);
                    panic!("original graceful Leave refused");
                }
            },
        );
        assert!(guard.input.as_mut().unwrap().begin_graceful_quiesce());
        while guard.input.as_ref().unwrap().progress() != FoundationInputProgress::Quiesced {
            assert!(Instant::now() < prepare_end);
            assert_ne!(
                guard.input.as_mut().unwrap().advance(),
                FoundationInputProgress::Stopped
            );
            std::thread::sleep(Duration::from_millis(1));
        }
        if scenario == "prior-generation" {
            assert!(
                guard.input.as_ref().unwrap().engine.is_some(),
                "the prior turn keeps its actual engine account"
            );
        }
        guard
            .input
            .as_mut()
            .unwrap()
            .begin_graceful_wind_down(TurnKey("wind-down-0".into()), CutMethod::Flush)
            .unwrap();
        let end = Instant::now() + Duration::from_secs(5);
        let mut join = None;
        let mut ticket = None;
        let mut event = None;
        let mut held_author = None;
        loop {
            assert!(
                Instant::now() < end,
                "wind-down never reached force boundary {scenario}"
            );
            let input = guard.input.as_mut().unwrap();
            if input.progress() == FoundationInputProgress::ManagementPending
                || input.serving.graceful.as_ref().unwrap().forced
            {
                break;
            }
            assert_ne!(
                input.progress(),
                FoundationInputProgress::Stopped,
                "{:?}",
                input.detail()
            );
            let phase = input
                .serving
                .graceful
                .as_ref()
                .unwrap()
                .wind_down
                .as_ref()
                .unwrap()
                .test_stage();
            let stop = match scenario {
                "unstarted" | "start-author" | "held-start" | "held-start-resume" => {
                    phase == "Start"
                }
                "instruction" => phase == "Instruction",
                "placed" => phase == "Placed",
                "generated" => phase == "Generated",
                "repair" => input.serving.owner.context.repair.is_some(),
                "restatement" => input.wind_down_progress() == Some(W::Restatement),
                "closing"
                | "close-author"
                | "expiry-close-recorded"
                | "held-close"
                | "held-close-resume" => phase == "Closing",
                _ => false,
            };
            if stop {
                if matches!(
                    scenario,
                    "held-start" | "held-start-resume" | "held-close" | "held-close-resume"
                ) {
                    held_author = Some(retain_held_turn(input, scenario.ends_with("-resume")));
                    join = Some(joining(socket, 75, 100));
                } else if scenario == "expiry-close-recorded" {
                    let mut io = CloseClock {
                        reads: 0,
                        // The seventh reading is the post-Recorded live check
                        // on this unpressured path. The receipt assertions below
                        // fail if that boundary moves or the close never lands.
                        cut: 7,
                        deadline: input
                            .serving
                            .owner
                            .context
                            .deadlines
                            .get(PumpDeadline::WindDown)
                            .unwrap()
                            .deadline_ns(),
                    };
                    let progress = input.advance_with_io(&mut io);
                    let context = &input.serving.owner.context;
                    assert_eq!(io.reads, io.cut);
                    assert_eq!(progress, FoundationInputProgress::LeavePending);
                    assert!(context.completed_author.is_some());
                    assert!(context.open_turn.is_none());
                    assert!(
                        input
                            .serving
                            .graceful
                            .as_ref()
                            .unwrap()
                            .wind_down
                            .as_ref()
                            .unwrap()
                            .test_records()
                            .1
                            .is_none()
                    );
                } else if author {
                    pause.store(true, Ordering::SeqCst);
                    while !paused.load(Ordering::SeqCst) {
                        assert!(Instant::now() < end);
                        std::thread::sleep(Duration::from_millis(1));
                    }
                    let context = &mut input.serving.owner.context;
                    let mut clock = FakePumpIo::new(context.last_ns);
                    assert!(crate::flow::context::worker_startup::input::management::stop::tests::fill_trace(context, &mut clock) > 0);
                    let mut io = JoinDuringWait { socket, peer: None };
                    assert_eq!(
                        input.advance_with_io(&mut io),
                        FoundationInputProgress::ManagementPending
                    );
                    let pending = input.serving.owner.context.pending_author().unwrap();
                    assert_eq!(
                        pending.event.kind,
                        if scenario == "start-author" {
                            serving::Kind::TurnStarted
                        } else {
                            serving::Kind::TurnClosed
                        }
                    );
                    ticket = Some(pending.ticket);
                    event = Some(
                        serde_json::to_value(
                            input.serving.owner.context.barrier.pending().unwrap(),
                        )
                        .unwrap(),
                    );
                    join = io.peer;
                } else {
                    join = Some(joining(socket, 75, 100));
                }
                break;
            }
            match input.wind_down_progress().unwrap() {
                W::Placed => assert!(input.generate_wind_down()),
                W::Generated => assert!(input.repair_wind_down(&FAMILY)),
                W::Restatement => assert!(input.place_wind_down_restatement()),
                W::CorrectionPlaced => assert!(input.generate_wind_down_correction()),
                W::Effective => {
                    if scenario == "pair-filing" {
                        std::fs::write(
                            root.join("wc-arm-pair"),
                            b"next receipt is the original pair",
                        )
                        .unwrap();
                        wait_file(root, "wc-pair-armed");
                    }
                    assert!(input.finish_wind_down_pass());
                }
                W::PassComplete => assert!(input.close_wind_down()),
                W::Pending => {
                    input.advance();
                }
                other => panic!("force boundary {scenario} reached unexpected {other:?}"),
            }
        }
        if !expiry {
            while guard.input.as_ref().unwrap().progress()
                != FoundationInputProgress::ManagementPending
            {
                assert!(Instant::now() < end);
                assert_ne!(
                    guard.input.as_mut().unwrap().wait_for_wake(),
                    FoundationInputProgress::Stopped
                );
            }
            guard.management = Some(guard.input.take().unwrap().management().ok().unwrap());
            while guard.management.as_ref().unwrap().progress()
                == weaver_harness::FoundationManagementProgress::Pending
            {
                guard.management.as_mut().unwrap().advance();
            }
            guard.input = Some(
                match guard.management.take().unwrap().join_graceful_leave() {
                    Ok(input) => input,
                    Err(owner) => {
                        guard.management = Some(owner);
                        panic!("actual wind-down JoinLeave refused");
                    }
                },
            );
        }
        let input = guard.input.as_ref().unwrap();
        let leave = input.serving.graceful.as_ref().unwrap();
        assert!(leave.forced);
        assert_eq!(
            leave.forced_by,
            (!expiry).then_some(weaver_types::Cause { uid: 75 })
        );
        let original = std::ptr::from_ref(leave.wind_down.as_deref().unwrap()) as usize;
        let evidence = Evidence {
            waits: input.serving.owner.context.deadlines.entries().collect(),
            turn: input.serving.owner.context.open_turn.clone(),
            ticket,
            event,
            completed_author: input.serving.owner.context.completed_author,
            held_author,
        };
        assert!(!root.join("ready-rollback-lower.json").exists());
        let actual = match guard.input.take().unwrap().graceful_leave_handoff() {
            Ok(actual) => actual,
            Err(input) => {
                let detail = input.detail();
                guard.input = Some(input);
                panic!("wind-down handoff refused: {detail:?}");
            }
        };
        assert_eq!(
            std::ptr::from_ref(
                actual
                    .inner
                    .graceful_tail
                    .as_ref()
                    .unwrap()
                    .wind_down
                    .as_deref()
                    .unwrap()
            ) as usize,
            original
        );
        pause.store(false, Ordering::SeqCst);
        done.store(true, Ordering::SeqCst);
        let other = responder.join().unwrap();
        let joins = join
            .or(other)
            .into_iter()
            .map(|peer| JoinReply {
                peer,
                ordinal: 100,
                answer: None,
            })
            .collect();
        (actual, primary, recorder.join().unwrap(), joins, evidence)
    });
    Handoff {
        actual: LeaveHandoff::Graceful(actual),
        peer: Some(primary),
        socket: socket.into(),
        before,
        joins,
        conversion: None,
        quiesce: None,
        wind_down: Some(evidence),
    }
}

pub(super) fn assert_tail(
    owner: &mut ForcedGuard,
    rows: &[serde_json::Value],
    joins: &[JoinReply],
    filings: &[s::StateFrame],
    root: &Path,
    mode: &str,
    evidence: &Evidence,
) {
    let scenario = case(mode);
    let unstarted = matches!(scenario, "unstarted" | "held-start");
    let sequence =
        |row: &serde_json::Value| row["sequence"].as_str().unwrap().parse::<u64>().unwrap();
    let started: Vec<_> = rows
        .iter()
        .filter(|row| row["kind"] == "turn.started" && row["turn"] == "wind-down-0")
        .collect();
    let closed: Vec<_> = rows
        .iter()
        .filter(|row| row["kind"] == "turn.closed" && row["turn"] == "wind-down-0")
        .collect();
    assert_eq!(started.len(), usize::from(!unstarted));
    assert_eq!(closed.len(), usize::from(!unstarted));
    let confirmed = rows
        .iter()
        .find(|row| row["kind"] == "save_point" && row["payload"]["phase"] == "confirmed")
        .unwrap();
    if !unstarted {
        assert!(
            sequence(started[0]) < sequence(closed[0]) && sequence(closed[0]) < sequence(confirmed)
        );
        if matches!(
            scenario,
            "close-author" | "expiry-close-recorded" | "held-close-resume"
        ) {
            assert_eq!(closed[0]["payload"]["close"], "clean");
        } else {
            assert_eq!(closed[0]["payload"]["reason"], "unload");
            assert_eq!(closed[0]["payload"]["cause"]["uid"], 74);
        }
    }
    let instruction = !matches!(
        scenario,
        "unstarted" | "start-author" | "instruction" | "held-start" | "held-start-resume"
    );
    assert_eq!(filings.iter().filter(|frame| matches!(frame,s::StateFrame::File { filing,.. } if filing.text=="Read summary.")).count(),usize::from(instruction));
    let cancel = matches!(
        scenario,
        "generation" | "expiry-generation" | "prior-generation" | "correction-generation"
    );
    assert_eq!(root.join("wc-cancel.json").exists(), cancel);
    if matches!(
        scenario,
        "unstarted"
            | "start-author"
            | "instruction"
            | "held-filing"
            | "measure"
            | "placed"
            | "held-start"
            | "held-start-resume"
    ) {
        assert!(!root.join("wc-generation-0").exists());
    }
    let unload = rows.last().unwrap();
    assert_eq!(unload["kind"], "unload");
    assert_eq!(
        unload["payload"]["forced_by"],
        if matches!(scenario, "expiry-generation" | "expiry-close-recorded") {
            serde_json::Value::Null
        } else {
            serde_json::json!({"uid":75})
        }
    );
    assert_eq!(
        joins.len(),
        usize::from(!matches!(
            scenario,
            "expiry-generation" | "expiry-close-recorded"
        ))
    );
    assert!(joins.iter().all(|join| join.answer.is_some()));
    let inner = owner.0.test_inner();
    let (_, handoff) = inner.test_account();
    let work = handoff
        .graceful_tail
        .as_ref()
        .unwrap()
        .wind_down
        .as_ref()
        .unwrap();
    assert!(work.settled_for_leave());
    assert_eq!(
        work.test_records().0.map(|seq| seq.0),
        started.first().map(|row| sequence(row))
    );
    assert_eq!(
        work.test_records().1.map(|seq| seq.0),
        closed.first().map(|row| sequence(row))
    );
    if scenario == "placed" {
        assert!(work.test_cancelled_placement());
    }
    if let Some(expected) = &evidence.held_author {
        match scenario {
            "held-start" => assert_eq!(work.test_unadmitted_author().as_ref(), Some(expected)),
            "held-close" => assert_eq!(work.test_unadmitted_close().as_ref(), Some(expected)),
            _ => {
                assert!(work.test_unadmitted_author().is_none());
                assert!(work.test_unadmitted_close().is_none());
            }
        }
    }
    assert!(inner.test_context().wind_down.is_none() && inner.test_context().open_turn.is_none());
    assert!(inner.test_context().input.snapshot().is_none());
    if scenario == "pair-filing" {
        assert!(
            filings
                .iter()
                .any(|frame| matches!(frame, s::StateFrame::FilePair { .. }))
        );
    }
}
