//! Actual successful worker Ready followed by root's rollback before input.
use super::*;
use crate::flow::clock::AbsoluteBound;
use crate::flow::context::failed_enter_driver::ChildState;
use crate::flow::context::worker_startup::FoundationRun;
use crate::flow::pump::PumpDeadline;
use std::os::fd::OwnedFd;
use std::sync::Arc;
use weaver_harness::{FoundationEnteredRollbackHandoff, FoundationEnteredRollbackProgress};

pub(in crate::flow::context::worker_startup) fn is_mode(mode: &str) -> bool {
    mode.contains("-session-ready-rollback")
}

pub(super) fn gate(mode: &str, root: &Path, fd: i32) {
    // Ready was produced by the ordinary gate fixture. Only the queued case
    // offers content, after the actual root rollback has won management.
    if mode.ends_with("-nonpristine") {
        packet!(fd, &frame(0, false));
    } else if mode.ends_with("-queued") {
        wait_file(root, "ready-rollback-queue");
        for index in 0..2 {
            packet!(fd, &frame(index, false));
        }
        std::fs::write(root.join("ready-rollback-queued"), b"sent").unwrap();
    }
    let mut bytes = vec![0; weaver_types::MAX_ENVELOPE_BYTES];
    let n = recv(fd, &mut bytes, MsgFlags::empty()).unwrap_or(0);
    if n == 0 {
        return;
    }
    let lower: OrganEnvelope = serde_json::from_slice(&bytes[..n]).unwrap();
    assert_eq!(lower.position, Position::Open);
    assert_eq!(lower.exchange.opener, weaver_types::Opener::Harness);
    assert_eq!(lower.payload, Payload::Directive(LifecycleDirective::Lower));
    std::fs::write(root.join("ready-rollback-lower.json"), &bytes[..n]).unwrap();
    packet!(
        fd,
        &OrganEnvelope {
            exchange: lower.exchange,
            position: Position::Close,
            payload: Payload::Answer(LifecycleAnswer::GateStopped),
        }
    );
}

pub(in crate::flow::context::worker_startup) struct Handoff {
    pub(super) actual: Box<FoundationEnteredRollbackHandoff>,
    pub(super) peer: OwnedFd,
    pub(super) origin: Arc<()>,
    pub(super) enter: AbsoluteBound,
    pub(super) cause: weaver_types::Cause,
}

pub(in crate::flow::context::worker_startup) fn handoff(
    serving: FoundationServing<'_>,
    socket: &Path,
    root: &Path,
    mode: &str,
    member: &mut Member,
) -> Option<Handoff> {
    let origin = Arc::clone(&serving.owner.context.read_origin);
    let enter = serving.owner.enter;
    let cause = serving
        .owner
        .resident
        .evidence
        .startup
        .original_enter
        .as_ref()
        .unwrap()
        .envelope()
        .enter()
        .cause;
    assert!(serving.owner.ready.sent());
    assert!(
        serving
            .owner
            .ready
            .sent_at()
            .is_some_and(|at| { at >= enter.began_ns() && !enter.expired(at) })
    );
    assert_eq!(serving.owner.context.retired_enter, Some(enter));
    assert!(
        serving
            .owner
            .context
            .deadlines
            .get(PumpDeadline::Enter)
            .is_none()
    );
    assert!(!serving.owner.context.ever_started_turn);
    if mode.ends_with("-late") {
        assert!(
            enter.expired(crate::flow::clock::monotonic_ns().unwrap()),
            "root actually observed Ready after its Enter window"
        );
    }
    let mut guard = SessionGuard(Some(Session::begin(serving, &FAMILY, CutMethod::Flush)));
    if mode.ends_with("-nonpristine") {
        let done = AtomicBool::new(false);
        std::thread::scope(|scope| {
            let responder = scope.spawn(|| {
                while !done.load(Ordering::SeqCst) {
                    member.step_live("rollback-ready");
                    std::thread::sleep(Duration::from_millis(1));
                }
            });
            let _stop = StopThreads(&done);
            drive(guard.0.as_mut().unwrap(), Progress::Accepted);
            done.store(true, Ordering::SeqCst);
            responder.join().unwrap();
        });
    }
    let peer = management(
        socket,
        LifecycleDirective::Leave {
            cause: weaver_types::Cause { uid: 74 },
            forced: true,
            rollback: !mode.ends_with("-nonrollback"),
        },
    );
    drive(guard.0.as_mut().unwrap(), Progress::DirectivePending);
    assert!(
        read_packet(&peer).is_none(),
        "the pending directive is not Left"
    );
    if mode.ends_with("-queued") {
        std::fs::write(root.join("ready-rollback-queue"), b"queue").unwrap();
        wait_file(root, "ready-rollback-queued");
    }
    let mut management = match guard.0.take().unwrap().into_owner() {
        Err(owner) => owner,
        Ok(input) => {
            let _guard = Guard {
                input: Some(input),
                management: None,
            };
            panic!("actual rollback must retain its management owner");
        }
    };
    if mode.ends_with("-held") {
        // These are the existing unadmitted-input instruments, not tickets or
        // success proofs. Each original must return before Leave can be armed.
        use crate::flow::context::held_input_tests::{HOLDS, hold};
        for held in HOLDS {
            let originals = hold(management.input.serving.owner.context, held);
            management = rejected(management, &peer, &origin, enter, root);
            originals.assert_held(management.input.serving.owner.context);
            originals.take(management.input.serving.owner.context);
        }
    }
    if mode.ends_with("-nonrollback") || mode.ends_with("-nonpristine") {
        if mode.ends_with("-nonpristine") {
            assert!(management.input.stage == super::super::Stage::Accepted);
            assert!(management.input.token.is_some());
            assert!(management.input.received.is_some());
        }
        // Retrying the actual rejected owner must not consume a directive,
        // frame or acceptance token, or manufacture cleanup authority.
        for _ in 0..2 {
            management = rejected(management, &peer, &origin, enter, root);
        }
        let _guard = Guard {
            input: None,
            management: Some(management),
        };
        return None;
    }
    let actual = match management.rollback_handoff() {
        Ok(handoff) => handoff,
        Err(management) => {
            let _guard = Guard {
                input: None,
                management: Some(management),
            };
            panic!("the pristine successful Ready must yield its actual rollback handoff");
        }
    };
    assert!(Arc::ptr_eq(&actual.origin, &origin));
    assert_eq!(actual.enter, enter);
    assert!(actual.ready.as_ref().unwrap().sent());
    assert_eq!(actual.message.envelope.exchange.opener, Opener::Admin);
    assert_eq!(actual.message.envelope.exchange.ordinal, 99);
    assert_eq!(
        actual.message.envelope.payload,
        Payload::Directive(LifecycleDirective::Leave {
            cause: weaver_types::Cause { uid: 74 },
            forced: true,
            rollback: true,
        })
    );
    Some(Handoff {
        actual,
        peer,
        origin,
        enter,
        cause,
    })
}

fn rejected<'a>(
    management: Box<weaver_harness::FoundationManagement<'a>>,
    peer: &OwnedFd,
    origin: &Arc<()>,
    enter: AbsoluteBound,
    root: &Path,
) -> Box<weaver_harness::FoundationManagement<'a>> {
    let identity = std::ptr::from_ref(&*management);
    let frame = management
        .input
        .received
        .as_ref()
        .map(|received| (received.raw_bytes().as_ptr(), received.raw_bytes().to_vec()));
    let token = management
        .input
        .token
        .as_ref()
        .map(|token| (std::ptr::from_ref(token), format!("{token:?}")));
    let recorded = management.input.serving.owner.context.last_author_sequence;
    let retained = match management.rollback_handoff() {
        Err(retained) => retained,
        Ok(mut handoff) => {
            crate::flow::context::worker_startup::tests::reap(handoff.arms.child.as_mut().unwrap());
            panic!("ineligible owner transferred its actual Ready and gate");
        }
    };
    assert_eq!(std::ptr::from_ref(&*retained), identity);
    assert_eq!(
        retained.progress(),
        weaver_harness::FoundationManagementProgress::DirectivePending
    );
    assert_eq!(
        retained
            .input
            .received
            .as_ref()
            .map(|received| { (received.raw_bytes().as_ptr(), received.raw_bytes().to_vec()) }),
        frame
    );
    assert_eq!(
        retained
            .input
            .token
            .as_ref()
            .map(|token| { (std::ptr::from_ref(token), format!("{token:?}")) }),
        token
    );
    let context = &retained.input.serving.owner.context;
    assert!(Arc::ptr_eq(&context.read_origin, origin));
    assert_eq!(context.retired_enter, Some(enter));
    assert!(context.deadlines.get(PumpDeadline::Enter).is_none());
    assert!(context.deadlines.get(PumpDeadline::Leave).is_none());
    assert_eq!(context.last_author_sequence, recorded);
    assert!(context.failure().is_none());
    assert!(context.flow.halt().is_none());
    assert!(!context.ever_started_turn);
    assert_eq!(
        retained.input.serving.owner.gate_child.state(),
        ChildState::Running
    );
    assert!(!root.join("ready-rollback-lower.json").exists());
    assert!(read_packet(peer).is_none());
    retained
}

pub(in crate::flow::context::worker_startup) fn reject_foreign(
    foreign: Box<FoundationRun>,
    handoff: Handoff,
) -> (Box<FoundationRun>, Handoff) {
    let Handoff {
        actual,
        peer,
        origin,
        enter,
        cause,
    } = handoff;
    let run_identity = std::ptr::from_ref(&*foreign);
    let handoff_identity = std::ptr::from_ref(&*actual);
    let foreign_origin = Arc::clone(&foreign.context.read_origin);
    let foreign_enter = foreign.context.deadlines.get(PumpDeadline::Enter);
    let original = foreign.original.as_ref().unwrap().wire().to_vec();
    assert!(!Arc::ptr_eq(&foreign_origin, &origin));
    let (foreign, actual) = foreign
        .entered_rollback(actual)
        .err()
        .expect("a foreign actual admission cannot consume the successful Ready seat");
    assert_eq!(std::ptr::from_ref(&*foreign), run_identity);
    assert_eq!(std::ptr::from_ref(&*actual), handoff_identity);
    assert!(Arc::ptr_eq(&foreign.context.read_origin, &foreign_origin));
    assert_eq!(
        foreign.context.deadlines.get(PumpDeadline::Enter),
        foreign_enter
    );
    assert_eq!(foreign.original.as_ref().unwrap().wire(), original);
    assert!(foreign.context.deadlines.get(PumpDeadline::Leave).is_none());
    assert!(foreign.listener.is_some());
    assert!(foreign.resources.classifier_child().is_none());
    assert!(foreign.resources.spu_child().is_none());
    assert!(Arc::ptr_eq(&actual.origin, &origin));
    assert_eq!(actual.enter, enter);
    assert!(actual.ready.as_ref().unwrap().sent());
    assert_eq!(
        actual.arms.child.as_ref().unwrap().state(),
        ChildState::Running
    );
    assert!(read_packet(&peer).is_none());
    (
        foreign,
        Handoff {
            actual,
            peer,
            origin,
            enter,
            cause,
        },
    )
}

pub(in crate::flow::context::worker_startup) fn finish(
    run: Box<FoundationRun>,
    handoff: Handoff,
    member: &mut Member,
    reader: &mut UnixStream,
    root: &Path,
    mode: &str,
) {
    if mode.contains("-edge-") {
        return super::ready_rollback_edges::finish(run, handoff, member, reader, root, mode);
    }
    let Handoff {
        actual,
        peer,
        origin,
        enter,
        cause,
    } = handoff;
    assert!(Arc::ptr_eq(&run.context.read_origin, &origin));
    assert_eq!(run.context.retired_enter, Some(enter));
    let leave = actual.leave;
    let original_raise = actual.gate_raise.clone();
    assert_eq!(run.context.deadlines.get(PumpDeadline::Leave), Some(leave));
    assert!(run.context.deadlines.get(PumpDeadline::Enter).is_none());
    let before = member.requests.clone();
    let done = AtomicBool::new(false);
    reader.set_nonblocking(true).unwrap();
    let (mut owner, bytes) = std::thread::scope(|scope| {
        let responder = scope.spawn(|| {
            while !done.load(Ordering::SeqCst) {
                member.step_live("rollback-ready");
                std::thread::sleep(Duration::from_millis(1));
            }
        });
        let recorder = scope.spawn(|| {
            let mut bytes = vec![];
            while !done.load(Ordering::SeqCst) {
                let _ = reader.read_to_end(&mut bytes);
                std::thread::sleep(Duration::from_millis(1));
            }
            let _ = reader.read_to_end(&mut bytes);
            bytes
        });
        let _stop = StopThreads(&done);
        let owner = match worker::entered_rollback(run, actual) {
            Ok(owner) => owner,
            Err((mut run, mut handoff)) => {
                crate::flow::context::worker_startup::tests::reap(
                    handoff.arms.child.as_mut().unwrap(),
                );
                run.resources.reap_startup_test_children();
                panic!("the original run rejected its successful Ready rollback handoff");
            }
        };
        done.store(true, Ordering::SeqCst);
        responder.join().unwrap();
        (owner, recorder.join().unwrap())
    });
    assert_eq!(owner.progress(), FoundationEnteredRollbackProgress::Closed);
    assert!(!owner.abandoned(), "{:?}", owner.detail());
    assert!(owner.detail().is_none(), "{:?}", owner.detail());
    let (run, account) = owner.test_account();
    assert!(Arc::ptr_eq(&run.context.read_origin, &origin));
    assert!(Arc::ptr_eq(&account.origin, &origin));
    assert_eq!(account.enter, enter);
    assert_eq!(account.leave, leave);
    assert_eq!(run.context.retired_enter, Some(enter));
    assert!(run.context.deadlines.get(PumpDeadline::Enter).is_none());
    assert!(run.context.failure().is_none());
    assert!(run.context.flow.halt().is_none());
    assert!(run.context.phase == crate::flow::context::RecordingPhase::Closed);
    assert!(!run.context.ever_started_turn);
    assert!(matches!(
        account.arms.child.as_ref().unwrap().state(),
        ChildState::Reaped(nix::sys::wait::WaitStatus::Exited(_, 0))
    ));
    assert!(matches!(
        run.resources.classifier_child(),
        Some(ChildState::Reaped(nix::sys::wait::WaitStatus::Exited(_, 0)))
    ));
    assert!(matches!(
        run.resources.spu_child(),
        Some(ChildState::Reaped(nix::sys::wait::WaitStatus::Exited(_, 0)))
    ));
    let gate = owner.test_gate().unwrap();
    let lower = gate.bound();
    assert_eq!(lower.began_ns(), leave.began_ns());
    assert_eq!(
        lower.deadline_ns(),
        (leave.began_ns() + weaver_types::LOWER_BOUND_MS * 1_000_000).min(leave.deadline_ns())
    );
    let (request, sent, sent_at) = gate.test_lower().unwrap();
    assert!(sent);
    assert!(sent_at.is_some_and(|at| at >= leave.began_ns() && !lower.expired(at)));
    assert_eq!(
        request.exchange.ordinal,
        original_raise.exchange.ordinal + 1
    );
    assert_eq!(
        serde_json::from_slice::<OrganEnvelope>(
            &std::fs::read(root.join("ready-rollback-lower.json")).unwrap()
        )
        .unwrap(),
        *request
    );
    let reply = gate.test_replies().1.unwrap().envelope();
    assert_eq!(reply.exchange, request.exchange);
    assert_eq!(reply.payload, Payload::Answer(LifecycleAnswer::GateStopped));
    assert!(gate.test_disposition().1);
    let mut expected = before;
    expected.push("grants".into());
    assert_eq!(member.requests, expected);
    member.assert_closed_without_request();
    let answer: OrganEnvelope = serde_json::from_slice(
        &read_packet(&peer).expect("actual worker rollback must return Left"),
    )
    .unwrap();
    assert_eq!(answer.exchange, account.message.envelope.exchange);
    assert_eq!(answer.position, Position::Close);
    assert_eq!(
        answer.payload,
        Payload::Answer(LifecycleAnswer::Left {
            save_point: None,
            forced: true,
            no_state: false,
        })
    );
    let recorded = weaver_trace::Sequence(owner.unload_sequence().expect("actual Unload receipt"));
    assert_eq!(run.context.last_author_sequence, Some(recorded));
    let rows = trace_through(
        reader,
        bytes,
        recorded,
        Instant::now() + Duration::from_secs(3),
    )
    .unwrap();
    assert_eq!(rows.iter().filter(|row| row["kind"] == "load").count(), 1);
    assert_eq!(rows.iter().filter(|row| row["kind"] == "unload").count(), 1);
    assert!(rows.iter().all(|row| !matches!(
        row["kind"].as_str(),
        Some("turn.started" | "save_point" | "fault")
    )));
    let load = rows.iter().find(|row| row["kind"] == "load").unwrap();
    let unload = rows.last().unwrap();
    assert_eq!(unload["kind"], "unload");
    assert_eq!(unload["sequence"], serde_json::to_value(recorded).unwrap());
    assert_eq!(
        unload["payload"]["cause"],
        serde_json::to_value(cause).unwrap()
    );
    assert_eq!(unload["payload"]["cause"], load["payload"]["cause"]);
    assert_eq!(unload["payload"]["forced"], true);
    assert_eq!(unload["payload"]["release"]["spu"], "confirmed");
    assert_eq!(unload["payload"]["release"]["member"], "closed");
    let refusals: Vec<_> = rows.iter().filter(|row| row["kind"] == "refusal").collect();
    assert_eq!(
        refusals.len(),
        if mode.ends_with("-queued") { 2 } else { 0 }
    );
    for row in refusals {
        assert_eq!(
            row["payload"],
            serde_json::to_value(weaver_types::RefusalRecord::Lifecycle {
                asked: weaver_types::LifecycleAsk::Leave,
                refusal: weaver_types::LifecycleRefusal::Unloading,
            })
            .unwrap()
        );
        assert!(row.get("turn").is_none_or(serde_json::Value::is_null));
    }
    assert_eq!(owner.advance(), FoundationEnteredRollbackProgress::Closed);
    assert!(read_packet(&peer).is_none(), "Left is sent only once");
}
