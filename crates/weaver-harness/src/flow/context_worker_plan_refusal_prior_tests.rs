//! Suspend a real earlier author, then resume it only through its worker owner.
use super::*;
use std::sync::atomic::{AtomicBool, Ordering};

struct Done<'a>(&'a AtomicBool);
impl Drop for Done<'_> {
    fn drop(&mut self) {
        self.0.store(true, Ordering::SeqCst);
    }
}

fn hold(input: &mut FoundationInput<'_>, flags: bool) -> u64 {
    if !flags && !input.plan_pending() && input.serving.save.is_none() {
        pressure::fill(input.serving.owner.context);
    }
    let bound = AbsoluteBound::after(MonotonicIo.now().unwrap(), 100_000_000).unwrap();
    input
        .serving
        .owner
        .context
        .arm(PumpDeadline::Label, bound, &mut MonotonicIo)
        .unwrap();
    loop {
        let result = if flags {
            input.advance_flags(None, &mut MonotonicIo)
        } else if input.plan_pending() {
            input.advance_plan(None, &mut MonotonicIo)
        } else if input.serving.save.is_some() {
            input.advance_save(&mut MonotonicIo, false).map(|_| ())
        } else {
            input.step_with_peer(None, &mut MonotonicIo)
        };
        match result {
            Ok(()) => (),
            Err(RunFailure::Wait {
                key: PumpDeadline::Label,
                bound: actual,
            }) if actual == bound => break,
            other => panic!("earlier author did not retain a recoverable wait: {other:?}"),
        }
    }
    let context = &mut input.serving.owner.context;
    context.complete_wait(PumpDeadline::Label).unwrap();
    assert!(context.failure().is_none());
    context
        .pending_author()
        .expect("actual earlier author ticket under recorder pressure")
        .ticket
}

fn refuse(guard: &mut Guard<'_>, reader: &mut UnixStream, socket: &Path, ticket: u64) {
    let earlier_plan = guard.owned_input().plan_pending();
    let earlier = format!(
        "{:?}",
        guard
            .owned_input()
            .serving
            .owner
            .context
            .pending_author()
            .unwrap()
            .event
    );
    let decision = guard
        .owned_input()
        .serving
        .owner
        .context
        .deadlines
        .get(PumpDeadline::Decision);
    let state = guard
        .owned_input()
        .serving
        .owner
        .context
        .deadlines
        .get(PumpDeadline::StateControl);
    let (peer, bytes) = request(socket, 0);
    let watch = Instant::now() + Duration::from_secs(5);
    while guard.input.as_mut().unwrap().advance() != FoundationInputProgress::ManagementPending {
        assert!(
            Instant::now() < watch,
            "earlier author did not yield management"
        );
        assert!(
            guard
                .owned_input()
                .serving
                .owner
                .context
                .failure()
                .is_none()
        );
    }
    guard.management = Some(guard.input.take().unwrap().management().ok().unwrap());
    let owner = guard.management.as_mut().unwrap();
    while owner.plan_refusal.is_none() {
        assert_eq!(
            owner.advance_with_io(&mut NonblockingIo { waits: 0 }),
            FoundationManagementProgress::Pending,
            "{:?}",
            owner.detail()
        );
        assert!(Instant::now() < watch, "busy opener was not refused");
    }
    let refusal = owner.plan_refusal.as_ref().unwrap();
    assert!(refusal.original.is_some());
    assert!(refusal.author.ticket().is_none());
    assert_eq!(owner.declined_plan.as_ref().unwrap().wire(), bytes);
    assert_eq!(
        owner
            .input
            .serving
            .owner
            .context
            .pending_author()
            .unwrap()
            .ticket,
        ticket
    );
    assert_eq!(
        format!(
            "{:?}",
            owner
                .input
                .serving
                .owner
                .context
                .pending_author()
                .unwrap()
                .event
        ),
        earlier
    );
    assert_eq!(
        owner
            .input
            .serving
            .owner
            .context
            .deadlines
            .get(PumpDeadline::Decision),
        decision
    );
    assert_eq!(
        owner
            .input
            .serving
            .owner
            .context
            .deadlines
            .get(PumpDeadline::StateControl),
        state
    );
    assert!(read_packet(&peer).is_none());
    let initial = pressure::drain(owner.input.serving.owner.context, reader);
    for _ in 0..100 {
        let progress = owner.advance_with_io(&mut NonblockingIo { waits: 0 });
        assert_ne!(
            progress,
            FoundationManagementProgress::Stopped,
            "{:?}",
            owner.detail()
        );
        if progress == FoundationManagementProgress::Answered {
            break;
        }
        std::thread::sleep(Duration::from_millis(1));
    }
    assert_eq!(
        owner.progress(),
        FoundationManagementProgress::Answered,
        "management must resume the earlier author's own continuation before its refusal"
    );
    let sequence = owner.plan_refusal.as_ref().unwrap().sequence.unwrap();
    assert_ne!(
        owner.plan_refusal.as_ref().unwrap().author.ticket(),
        Some(ticket)
    );
    let answer: OrganEnvelope =
        serde_json::from_slice(&read_packet(&peer).expect("refusal delivered")).unwrap();
    assert_eq!(
        answer.payload,
        Payload::Refusal(LifecycleRefusal::OutOfOrder)
    );
    let rows = trace_through(reader, initial, sequence, watch).unwrap();
    assert_eq!(
        rows.iter()
            .filter(|row| row["kind"] == "refusal"
                && row["payload"]["asked"]["ask"] == "plan_update"
                && row["payload"]["refusal"]["kind"] == "out_of_order")
            .count(),
        1
    );
    assert!(owner.input.serving.owner.context.pending_author().is_none());
    assert!(owner.input.serving.owner.context.waiting_content.is_none());
    assert!(owner.input.serving.owner.context.completed_author.is_none());
    assert!(owner.input.serving.owner.context.failure().is_none());
    if !earlier_plan {
        assert!(!owner.input.plan_pending());
    }
    guard.input = Some(guard.management.take().unwrap().finish().ok().unwrap());
}

pub(in crate::flow::context::worker_startup) fn turn(
    serving: FoundationServing<'_>,
    member: &mut Member,
    reader: &mut UnixStream,
    socket: &Path,
    task: bool,
    queued: bool,
) {
    reader.set_nonblocking(true).unwrap();
    let mut guard = Guard {
        input: Some(serving.input()),
        management: None,
    };
    let done = AtomicBool::new(false);
    std::thread::scope(|scope| {
        scope.spawn(|| {
            let end = Instant::now() + Duration::from_secs(10);
            while !done.load(Ordering::SeqCst) && Instant::now() < end {
                member.step_live("ready");
                std::thread::sleep(Duration::from_millis(1));
            }
        });
        let _done = Done(&done);
        let input = guard.input.as_mut().unwrap();
        let watch = Instant::now() + Duration::from_secs(3);
        while input.progress() != FoundationInputProgress::Accepted {
            assert_ne!(
                input.advance(),
                FoundationInputProgress::Stopped,
                "{:?}",
                input.detail()
            );
            assert!(Instant::now() < watch);
        }
        let turn = TurnKey("prior-author-turn".into());
        if task {
            input.start_task(turn.clone()).unwrap();
        } else {
            input.start_turn(turn.clone()).unwrap();
        }
        let ticket = hold(input, false);
        let save = queued.then(|| queue_save(&mut guard, socket));
        refuse(&mut guard, reader, socket, ticket);
        let input = guard.input.as_ref().unwrap();
        assert_eq!(input.turn.as_ref(), Some(&turn));
        if !task {
            assert_eq!(input.progress(), FoundationInputProgress::Turn);
            assert_eq!(input.seated.as_ref().unwrap().turn.0, turn.0);
        }
        if let Some((peer, bound)) = save {
            assert!(input.serving.save.is_some());
            assert_eq!(
                input
                    .serving
                    .owner
                    .context
                    .deadlines
                    .get(PumpDeadline::SaveQueued),
                Some(bound)
            );
            assert!(
                read_packet(&peer).is_none(),
                "the queued save starts no new boundary"
            );
        }
    });
}

pub(in crate::flow::context::worker_startup) fn flags(
    guard: &mut Guard<'_>,
    member: &mut Member,
    reader: &mut UnixStream,
    socket: &Path,
    plan: weaver_types::state_flow::PlanRef,
    after: weaver_types::state_flow::SourceBlockId,
) {
    reader.set_nonblocking(true).unwrap();
    pressure::fill(guard.input.as_mut().unwrap().serving.owner.context);
    guard
        .input
        .as_mut()
        .unwrap()
        .observe_flags(plan, after)
        .unwrap();
    let done = AtomicBool::new(false);
    std::thread::scope(|scope| {
        scope.spawn(|| {
            let end = Instant::now() + Duration::from_secs(10);
            while !done.load(Ordering::SeqCst) && Instant::now() < end {
                member.step_live("ready");
                std::thread::sleep(Duration::from_millis(1));
            }
        });
        let _done = Done(&done);
        let ticket = hold(guard.input.as_mut().unwrap(), true);
        refuse(guard, reader, socket, ticket);
        let input = guard.input.as_mut().unwrap();
        let watch = Instant::now() + Duration::from_secs(3);
        while input.flags_pending() {
            input.advance_flags(None, &mut MonotonicIo).unwrap();
            assert!(Instant::now() < watch);
        }
        assert!(
            input.take_observed_flags().is_some(),
            "prior flag completion remains with its original caller"
        );
    });
}

fn queue_save(guard: &mut Guard<'_>, socket: &Path) -> (OwnedFd, AbsoluteBound) {
    use weaver_types::state_flow::capture::{CaptureDirective, CaptureEnvelope, CapturePayload};
    let peer = dial(socket);
    let began = MonotonicIo.now().unwrap();
    let elapsed = guard
        .owned_input()
        .serving
        .owner
        .context
        .load_duty
        .as_ref()
        .unwrap()
        .persistence
        .save_wait
        .elapsed_ns
        .0;
    let bound = AbsoluteBound::after(began, elapsed).unwrap();
    let packet = CaptureEnvelope {
        exchange: ExchangeId {
            opener: Opener::Admin,
            ordinal: 712,
        },
        position: Position::Open,
        payload: CapturePayload::Directive(CaptureDirective::SavePoint {
            cause: Cause { uid: 0 },
            window: weaver_types::state_flow::SaveWindow {
                began_ns: began.into(),
                deadline_ns: bound.deadline_ns().into(),
            },
        }),
    };
    let bytes = serde_json::to_vec(&packet).unwrap();
    assert_eq!(
        send(peer.as_raw_fd(), &bytes, MsgFlags::MSG_NOSIGNAL).unwrap(),
        bytes.len()
    );
    let watch = Instant::now() + Duration::from_secs(3);
    while guard.input.as_mut().unwrap().advance() != FoundationInputProgress::ManagementPending {
        assert!(Instant::now() < watch);
    }
    guard.management = Some(guard.input.take().unwrap().management().ok().unwrap());
    while guard.management.as_mut().unwrap().advance()
        != FoundationManagementProgress::DirectivePending
    {
        assert!(Instant::now() < watch);
    }
    guard.input = Some(match guard.management.take().unwrap().queue_save() {
        Ok(input) => input,
        Err(owner) => {
            guard.management = Some(owner);
            panic!("queued save was not admitted");
        }
    });
    (peer, bound)
}

pub(in crate::flow::context::worker_startup) fn quiesce(
    serving: FoundationServing<'_>,
    member: &mut Member,
    reader: &mut UnixStream,
    socket: &Path,
) {
    reader.set_nonblocking(true).unwrap();
    let mut guard = Guard {
        input: Some(serving.input()),
        management: None,
    };
    let done = AtomicBool::new(false);
    std::thread::scope(|scope| {
        scope.spawn(|| {
            let end = Instant::now() + Duration::from_secs(10);
            while !done.load(Ordering::SeqCst) && Instant::now() < end {
                member.step_live("ready");
                std::thread::sleep(Duration::from_millis(1));
            }
        });
        let _done = Done(&done);
        let peer = dial(socket);
        let packet = OrganEnvelope {
            exchange: ExchangeId {
                opener: Opener::Admin,
                ordinal: 913,
            },
            position: Position::Open,
            payload: Payload::Directive(LifecycleDirective::Leave {
                cause: Cause { uid: 0 },
                forced: false,
                rollback: false,
            }),
        };
        let bytes = serde_json::to_vec(&packet).unwrap();
        assert_eq!(
            send(peer.as_raw_fd(), &bytes, MsgFlags::MSG_NOSIGNAL).unwrap(),
            bytes.len()
        );
        let watch = Instant::now() + Duration::from_secs(3);
        while guard.input.as_mut().unwrap().advance() != FoundationInputProgress::ManagementPending
        {
            assert!(Instant::now() < watch);
        }
        guard.management = Some(guard.input.take().unwrap().management().ok().unwrap());
        while guard.management.as_mut().unwrap().advance()
            != FoundationManagementProgress::DirectivePending
        {
            assert!(Instant::now() < watch);
        }
        guard.input = Some(
            match guard.management.take().unwrap().defer_graceful_leave() {
                Ok(input) => input,
                Err(owner) => {
                    guard.management = Some(owner);
                    panic!(
                        "graceful Leave was not admitted: {:?}",
                        guard.management.as_ref().unwrap().detail()
                    );
                }
            },
        );
        let input = guard.input.as_mut().unwrap();
        assert!(input.begin_graceful_quiesce());
        let ticket = hold(input, false);
        assert_eq!(
            input
                .serving
                .owner
                .context
                .pending_author()
                .unwrap()
                .event
                .kind,
            serving::Kind::Refusal
        );
        refuse(&mut guard, reader, socket, ticket);
        assert!(guard.input.as_ref().unwrap().has_graceful_leave());
        assert!(
            read_packet(&peer).is_none(),
            "accounting is not a Leave answer"
        );
    });
}

pub(in crate::flow::context::worker_startup) fn management_author(
    serving: FoundationServing<'_>,
    member: &mut Member,
    reader: &mut UnixStream,
    socket: &Path,
    save: bool,
) {
    reader.set_nonblocking(true).unwrap();
    let mut guard = Guard {
        input: Some(serving.input()),
        management: None,
    };
    pressure::fill(guard.input.as_mut().unwrap().serving.owner.context);
    let done = AtomicBool::new(false);
    std::thread::scope(|scope| {
        scope.spawn(|| {
            let end = Instant::now() + Duration::from_secs(10);
            while !done.load(Ordering::SeqCst) && Instant::now() < end {
                member.step_live("ready");
                std::thread::sleep(Duration::from_millis(1));
            }
        });
        let _done = Done(&done);
        let (peer, save_bound) = if save {
            let (peer, bound) = queue_save(&mut guard, socket);
            (peer, Some(bound))
        } else {
            use crate::flow::context::worker_startup::input::plan::tests as root;
            let peer = root::request(socket, "{ malformed original root plan }");
            root::accepted(&mut guard);
            (peer, None)
        };
        let ticket = hold(guard.input.as_mut().unwrap(), false);
        refuse(&mut guard, reader, socket, ticket);
        let input = guard.input.as_mut().unwrap();
        if let Some(bound) = save_bound {
            assert!(input.serving.save.is_some());
            assert_eq!(
                input
                    .serving
                    .owner
                    .context
                    .deadlines
                    .get(PumpDeadline::SaveQueued),
                Some(bound)
            );
            assert!(read_packet(&peer).is_none());
        } else {
            let watch = Instant::now() + Duration::from_secs(3);
            while input.plan_pending() {
                input.advance_plan(None, &mut MonotonicIo).unwrap();
                assert!(Instant::now() < watch);
            }
            let answer: OrganEnvelope =
                serde_json::from_slice(&read_packet(&peer).unwrap()).unwrap();
            assert_eq!(
                answer.exchange.ordinal, 2,
                "the original plan owns its own answer"
            );
        }
    });
}
