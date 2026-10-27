//! The demand origin shares the real capture owner, state stream and recorder.
//! These synthetic member answers do not qualify the admin publication path.
use super::*;
use crate::flow::report::CaptureState;

fn demand_window(run: &mut Fixture) -> s::SaveWindow {
    let began = run.clock.now().unwrap();
    let wait = run
        .context
        .load_duty
        .as_ref()
        .unwrap()
        .persistence
        .save_wait
        .elapsed_ns
        .0;
    s::SaveWindow {
        began_ns: began.into(),
        deadline_ns: began.checked_add(wait).unwrap().into(),
    }
}

fn demand<'a>(run: &'a mut Fixture, window: s::SaveWindow, received: u64) -> Parent<'a> {
    let (near, far) = socketpair(
        AddressFamily::Unix,
        SockType::SeqPacket,
        None,
        SockFlag::SOCK_NONBLOCK | SockFlag::SOCK_CLOEXEC,
    )
    .unwrap();
    let mut peer = LifecyclePeer::from_owned(near).unwrap();
    peer.enable_capture().unwrap();
    let exchange = ExchangeId {
        opener: Opener::Admin,
        ordinal: 29,
    };
    let capture = ServingCapture::queue_demand(
        &mut run.context,
        &peer,
        exchange.clone(),
        window,
        received,
        &mut run.clock,
    )
    .unwrap();
    Parent {
        member: Member::new(run),
        capture,
        peer,
        admin: far,
        exchange,
    }
}

#[test]
fn demand_capture_allocation_retains_its_request_until_an_explicit_turn_boundary() {
    let mut run = active();
    let window = demand_window(&mut run);
    let received = run.clock.now().unwrap();
    let next_id = run.context.next_capture_id.unwrap();
    let mut parent = demand(&mut run, window, received);
    assert_eq!(
        parent
            .capture
            .at_halt(&parent.member.run.context)
            .capture_id
            .0,
        next_id
    );
    assert_eq!(
        parent.capture.at_halt(&parent.member.run.context).state,
        CaptureState::QueueBoundary
    );
    for _ in 0..4 {
        parent.step().unwrap();
    }
    let records = parent.member.records(Kind::SavePoint);
    assert_eq!(records.len(), 1);
    assert_eq!(records[0]["payload"]["phase"], "requested");
    assert_eq!(records[0]["payload"]["cause"], "requested");
    assert_eq!(
        records[0]["payload"]["window"]["began_ns"],
        window.began_ns.0.to_string()
    );
    assert_eq!(
        records[0]["payload"]["window"]["deadline_ns"],
        window.deadline_ns.0.to_string()
    );
    assert!(records[0]["payload"]["tail_window"].is_null());
    assert!(parent.member.run.context.capture_snapshot().is_none());
    assert!(
        parent
            .member
            .run
            .context
            .deadlines
            .get(PumpDeadline::Leave)
            .is_none()
    );
    assert!(
        parent
            .member
            .run
            .context
            .deadlines
            .get(PumpDeadline::CaptureWindow)
            .is_none()
    );
    let mut bytes = [0; 4096];
    assert_eq!(
        recv(parent.admin.as_raw_fd(), &mut bytes, MsgFlags::MSG_DONTWAIT),
        Err(nix::errno::Errno::EAGAIN)
    );
    assert!(
        parent
            .capture
            .begin_demand_boundary(&mut parent.member.run.context, &mut parent.member.run.clock)
            .unwrap()
    );
    let beginning = parent.next();
    let CapturePayload::SavePointBeginning(beginning) = beginning.payload else {
        panic!("Beginning at boundary")
    };
    assert_eq!(beginning.window, window);
    assert_eq!(beginning.tail_window, None);
    assert_eq!(beginning.capture_id.0, next_id);
    assert_eq!(
        parent.member.run.context.next_capture_id,
        next_id.checked_add(1)
    );
    assert!(parent.member.run.context.capture_snapshot().is_none());
}

#[test]
fn demand_capture_expiry_at_author_admission_retains_its_exact_private_original() {
    use crate::flow::author::AuthoredSource;
    use crate::flow::context::capture_author::QueuedSaveAuthor;
    use std::sync::Arc;
    let mut run = active();
    let window = demand_window(&mut run);
    let received = run.clock.now().unwrap();
    let mut parent = demand(&mut run, window, received);
    let id = parent
        .capture
        .at_halt(&parent.member.run.context)
        .capture_id;
    let payload = p::SavePointRecord::Requested {
        capture_id: id.0.into(),
        window: p::SaveWindow {
            began_ns: window.began_ns.0.into(),
            deadline_ns: window.deadline_ns.0.into(),
        },
        tail_window: None,
        cause: p::PersistenceCause::Requested,
    };
    let event = || AuthoredEvent {
        kind: Kind::SavePoint,
        subsystem: Subsystem::Harness,
        turn: None,
        source: AuthoredSource::Runtime,
        causal_parent: None,
        payload: Some(Payload::SavePointRecord(Box::new(payload.clone()))),
    };
    let context = &mut parent.member.run.context;
    let claim = Arc::new(QueuedSaveAuthor::new(context, &payload));
    let other = Arc::new(QueuedSaveAuthor::new(context, &payload));
    parent
        .member
        .run
        .clock
        .advance_to(window.deadline_ns.0)
        .unwrap();
    let mut ports = PumpPorts {
        lifecycle: LifecyclePorts {
            leave_peer: Some(&mut parent.peer),
            ..Default::default()
        },
        gate: None,
        decode: None,
        label: None,
        io: &mut parent.member.run.clock,
    };
    let original = event();
    let bytes = format!("{original:?}");
    let failure = context
        .author_queued_save(&mut ports, &claim, original)
        .unwrap_err();
    assert!(matches!(
        failure.reason,
        RunFailure::Wait {
            key: PumpDeadline::SaveQueued,
            ..
        }
    ));
    assert!(failure.original.is_none());
    assert!(
        parent
            .capture
            .retain_queued_expiry(context, &failure.reason)
    );
    assert_eq!(
        format!("{:?}", context.waiting_content.as_ref().unwrap().event),
        bytes
    );
    assert!(Arc::ptr_eq(
        context.queued_save_author.as_ref().unwrap(),
        &claim
    ));
    let competitor = context.author_and_wait(&mut ports, event()).unwrap_err();
    assert_eq!(competitor.reason, RunFailure::Busy(HeldInput::Caller));
    assert!(competitor.original.is_some());
    let impostor = context
        .author_queued_save(&mut ports, &other, event())
        .unwrap_err();
    assert_eq!(impostor.reason, RunFailure::Busy(HeldInput::Caller));
    assert!(impostor.original.is_some());
    let generic = context.resume_author_and_wait(&mut ports).unwrap_err();
    assert_eq!(generic.reason, RunFailure::Busy(HeldInput::Caller));
    assert!(generic.original.is_none());
    let unclaimed = context
        .resume_claimed_author_and_wait(&mut ports, None)
        .unwrap_err();
    assert_eq!(unclaimed.reason, RunFailure::Busy(HeldInput::Caller));
    assert_eq!(
        format!("{:?}", context.waiting_content.as_ref().unwrap().event),
        bytes
    );
    let sequence = context.resume_queued_save(&mut ports, &claim).unwrap();
    assert_eq!(claim.sequence(), Some(sequence));
    assert!(other.sequence().is_none());
    assert!(context.queued_save_author.is_none());
    assert!(context.waiting_content.is_none());
    assert!(context.failure().is_none());
    assert!(context.capture_snapshot().is_none());
    assert_eq!(parent.member.records(Kind::SavePoint).len(), 1);
}

#[test]
fn demand_capture_accepts_an_expired_original_without_granting_a_boundary() {
    let mut run = active();
    let window = demand_window(&mut run);
    run.clock.advance_to(window.deadline_ns.0).unwrap();
    let mut parent = demand(&mut run, window, window.deadline_ns.0);
    assert_eq!(
        parent.capture.at_halt(&parent.member.run.context).state,
        CaptureState::QueueBoundary
    );
    assert!(
        parent
            .capture
            .begin_demand_boundary(&mut parent.member.run.context, &mut parent.member.run.clock)
            .is_err()
    );
    assert!(parent.member.run.context.halt().is_none());
    assert!(parent.member.run.context.capture_snapshot().is_none());
    assert!(
        parent
            .member
            .run
            .context
            .deadlines
            .get(PumpDeadline::CaptureWindow)
            .is_none()
    );
    assert!(parent.capture.report().is_none());
}

#[test]
fn demand_capture_requires_the_requested_receipt_and_no_open_turn_at_boundary() {
    let mut run = active();
    let window = demand_window(&mut run);
    let received = run.clock.now().unwrap();
    let mut parent = demand(&mut run, window, received);
    assert!(
        !parent
            .capture
            .begin_demand_boundary(&mut parent.member.run.context, &mut parent.member.run.clock)
            .unwrap()
    );
    parent.step().unwrap();
    parent.member.run.context.open_turn = Some(weaver_trace::TurnRef("same-current-turn".into()));
    assert!(
        !parent
            .capture
            .begin_demand_boundary(&mut parent.member.run.context, &mut parent.member.run.clock)
            .unwrap()
    );
    assert_eq!(
        parent.member.run.context.open_turn.as_ref().unwrap().0,
        "same-current-turn"
    );
    assert!(parent.member.run.context.capture_snapshot().is_none());
    assert!(
        parent
            .member
            .run
            .context
            .deadlines
            .get(PumpDeadline::CaptureWindow)
            .is_none()
    );
    assert_eq!(
        parent
            .member
            .run
            .context
            .deadlines
            .get(PumpDeadline::SaveQueued)
            .unwrap()
            .deadline_ns(),
        window.deadline_ns.0
    );
    parent.member.run.context.open_turn = None;
    assert!(
        parent
            .capture
            .begin_demand_boundary(&mut parent.member.run.context, &mut parent.member.run.clock)
            .unwrap()
    );
    assert!(
        parent
            .member
            .run
            .context
            .deadlines
            .get(PumpDeadline::SaveQueued)
            .is_none()
    );
}

#[test]
fn demand_capture_checks_the_original_receive_reading_and_loaded_duration_before_allocation() {
    for fault in [
        "future_began",
        "future_receive",
        "duration",
        "zero",
        "reversed",
        "opener",
    ] {
        let mut run = active();
        let mut window = demand_window(&mut run);
        let mut received = run.clock.now().unwrap();
        let mut exchange = ExchangeId {
            opener: Opener::Admin,
            ordinal: 29,
        };
        match fault {
            "future_began" => {
                window.began_ns.0 += 1;
                window.deadline_ns.0 += 1;
            }
            "future_receive" => received += 1,
            "duration" => window.deadline_ns.0 -= 1,
            "zero" => window.deadline_ns = window.began_ns,
            "reversed" => {
                window.began_ns.0 = window.deadline_ns.0 + 1;
            }
            "opener" => exchange.opener = Opener::Harness,
            _ => unreachable!(),
        }
        let (near, _far) = socketpair(
            AddressFamily::Unix,
            SockType::SeqPacket,
            None,
            SockFlag::SOCK_NONBLOCK | SockFlag::SOCK_CLOEXEC,
        )
        .unwrap();
        let mut peer = LifecyclePeer::from_owned(near).unwrap();
        peer.enable_capture().unwrap();
        let id = run.context.next_capture_id;
        let result = ServingCapture::queue_demand(
            &mut run.context,
            &peer,
            exchange,
            window,
            received,
            &mut run.clock,
        );
        assert!(result.is_err(), "{fault}");
        assert_eq!(run.context.next_capture_id, id, "{fault}");
        assert!(
            run.context
                .deadlines
                .get(PumpDeadline::SaveQueued)
                .is_none(),
            "{fault}"
        );
        assert!(
            run.context
                .deadlines
                .get(PumpDeadline::CaptureWindow)
                .is_none(),
            "{fault}"
        );
        assert!(run.context.capture_snapshot().is_none(), "{fault}");
        assert!(run.context.halt().is_some(), "{fault}");
    }
}

#[test]
fn demand_capture_completes_with_the_exact_saved_report_after_recorded_release() {
    let mut run = active();
    let window = demand_window(&mut run);
    let received = run.clock.now().unwrap();
    let loaded = run.context.open_start.clone().unwrap();
    let mut parent = demand(&mut run, window, received);
    parent.step().unwrap();
    assert!(
        parent
            .capture
            .begin_demand_boundary(&mut parent.member.run.context, &mut parent.member.run.clock)
            .unwrap()
    );
    let (beginning, artifact) = parent.prepared();
    assert_eq!(beginning.window, window);
    assert!(beginning.tail_window.is_none());
    parent.submit(CapturePayload::SavePointPrepared(artifact.clone()));
    assert!(matches!(
        parent.next().payload,
        CapturePayload::SavePointCommit(_)
    ));
    assert!(parent.capture.report().is_none());
    parent.submit(CapturePayload::SavePointPublished(artifact.clone()));
    let request = parent.request();
    assert!(
        matches!(request, s::StateFrame::Control { frame: s::StateControl::ReleaseCapture { capture_id }, .. } if capture_id == beginning.capture_id)
    );
    assert!(parent.capture.report().is_none());
    let released = parent.member.answer(
        &request,
        s::StateControlAnswer::CaptureReleased {
            capture_id: beginning.capture_id,
        },
    );
    parent.member.receive(&released).unwrap();
    let complete = parent.next();
    assert_eq!(complete.exchange, parent.exchange);
    assert_eq!(complete.position, Position::Close);
    let CapturePayload::Answer(s::capture::CaptureAnswer::SavePointTaken { report }) =
        complete.payload
    else {
        panic!("demand ends with SavePointTaken")
    };
    assert_eq!(parent.step().unwrap(), ServingCaptureProgress::Published);
    assert_eq!(parent.capture.report().unwrap(), report);
    assert_eq!(report.capture_id, beginning.capture_id);
    assert_eq!(report.start.seed_id, loaded.seed_id);
    assert_eq!(report.start.parent_artifact_id, Some(loaded.artifact_id));
    assert_eq!(report.start.artifact_id, artifact.artifact_id);
    assert_eq!(
        parent
            .member
            .run
            .context
            .open_start
            .as_ref()
            .unwrap()
            .artifact_id,
        report.start.parent_artifact_id.as_ref().unwrap().as_str()
    );
    assert!(parent.member.run.context.capture_snapshot().is_none());
    for key in [
        PumpDeadline::SaveQueued,
        PumpDeadline::CaptureWindow,
        PumpDeadline::Leave,
    ] {
        assert!(parent.member.run.context.deadlines.get(key).is_none());
    }
    let records = parent.member.records(Kind::SavePoint);
    assert_eq!(records.len(), 3);
    for record in records {
        assert_eq!(record["payload"]["cause"], "requested");
        assert!(record["payload"]["tail_window"].is_null());
        assert_eq!(
            record["payload"]["window"]["began_ns"],
            window.began_ns.0.to_string()
        );
        assert_eq!(
            record["payload"]["window"]["deadline_ns"],
            window.deadline_ns.0.to_string()
        );
    }
}

#[test]
fn demand_capture_final_reply_pressure_cannot_renew_the_window_or_grant_success() {
    let mut run = active();
    let window = demand_window(&mut run);
    let received = run.clock.now().unwrap();
    let mut parent = demand(&mut run, window, received);
    parent.step().unwrap();
    assert!(
        parent
            .capture
            .begin_demand_boundary(&mut parent.member.run.context, &mut parent.member.run.clock)
            .unwrap()
    );
    let (beginning, artifact) = parent.prepared();
    parent.submit(CapturePayload::SavePointPrepared(artifact.clone()));
    assert!(matches!(
        parent.next().payload,
        CapturePayload::SavePointCommit(_)
    ));
    parent.submit(CapturePayload::SavePointPublished(artifact));
    let request = parent.request();
    assert!(
        matches!(request, s::StateFrame::Control { frame: s::StateControl::ReleaseCapture { capture_id }, .. } if capture_id == beginning.capture_id)
    );
    let released = parent.member.answer(
        &request,
        s::StateControlAnswer::CaptureReleased {
            capture_id: beginning.capture_id,
        },
    );
    parent.member.receive(&released).unwrap();
    parent.pressure();
    for _ in 0..4 {
        assert_ne!(parent.step().unwrap(), ServingCaptureProgress::Published);
    }
    assert!(parent.capture.report().is_none());
    let bound = parent
        .member
        .run
        .context
        .deadlines
        .get(PumpDeadline::CaptureWindow)
        .unwrap();
    assert_eq!(bound.began_ns(), window.began_ns.0);
    assert_eq!(bound.deadline_ns(), window.deadline_ns.0);
    parent
        .member
        .run
        .clock
        .advance_to(window.deadline_ns.0)
        .unwrap();
    parent.drain_pressure();
    assert!(
        matches!(parent.step(), Err(RunFailure::Wait { key: PumpDeadline::CaptureWindow, bound: actual }) if actual == bound)
    );
    assert!(parent.capture.report().is_none());
    assert_eq!(
        parent
            .member
            .run
            .context
            .deadlines
            .get(PumpDeadline::CaptureWindow),
        Some(bound)
    );
    let mut bytes = [0; 4096];
    assert_eq!(
        recv(parent.admin.as_raw_fd(), &mut bytes, MsgFlags::MSG_DONTWAIT),
        Err(nix::errno::Errno::EAGAIN)
    );
}

#[test]
fn demand_capture_failed_queued_owner_keeps_its_identity_without_a_leave() {
    let mut run = active();
    let window = demand_window(&mut run);
    let received = run.clock.now().unwrap();
    let parent = demand(&mut run, window, received);
    let expected = parent.capture.at_halt(&parent.member.run.context);
    assert_eq!(expected.state, CaptureState::QueueBoundary);
    parent.member.run.context.fail(
        &mut parent.member.run.clock,
        FlowHaltReason::CarryFailed,
        HaltPoint::StateReply,
        "demand member failed while queued",
    );
    let failed = match parent
        .capture
        .into_failed(&parent.member.run.context, parent.peer)
    {
        Ok(failed) => failed,
        Err(_) => panic!("original failed demand custody must transfer"),
    };
    assert!(failed.matches(&parent.member.run.context));
    assert_eq!(failed.facts(), &expected);
    assert!(!failed.facts().commit_sent);
    assert!(failed.facts().published.is_none());
    assert!(
        parent
            .member
            .run
            .context
            .deadlines
            .get(PumpDeadline::Leave)
            .is_none()
    );
    assert!(parent.member.run.context.capture_snapshot().is_none());
}
