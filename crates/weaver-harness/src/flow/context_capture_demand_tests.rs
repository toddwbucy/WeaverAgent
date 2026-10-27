//! The demand origin shares the real capture owner, state stream and recorder.
//! These synthetic member answers do not qualify the admin publication path.
use super::*;
use crate::flow::report::CaptureState;

fn expire_demand_before_commit(finished: bool) {
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
    let id = if finished {
        parent.prepared().0.capture_id
    } else {
        parent.image().0.capture_id
    };
    parent
        .member
        .run
        .clock
        .advance_to(window.deadline_ns.0)
        .unwrap();
    let request = parent.request();
    assert!(
        matches!(request, s::StateFrame::Control { frame: s::StateControl::Abort { capture_id }, .. } if capture_id == id)
    );
    assert!(
        parent.capture.refusal_leg().is_none(),
        "cleanup has not yet proved Ready"
    );
    assert!(parent.member.run.context.capture_snapshot().is_some());
    assert!(
        !parent
            .capture
            .at_halt(&parent.member.run.context)
            .commit_sent
    );
    assert!(parent.capture.report().is_none());
    let reply = parent
        .member
        .answer(&request, s::StateControlAnswer::Aborted { capture_id: id });
    parent.member.receive(&reply).unwrap();
    for _ in 0..64 {
        if parent.step().unwrap() == ServingCaptureProgress::Refused {
            break;
        }
    }
    assert_eq!(parent.step().unwrap(), ServingCaptureProgress::Refused);
    assert_eq!(
        parent.capture.refusal_leg(),
        Some(if finished {
            SavePointLeg::Published
        } else {
            SavePointLeg::Finished
        })
    );
    assert!(parent.member.run.context.failure().is_none());
    assert!(parent.member.run.context.capture_snapshot().is_none());
    let records = parent.member.records(Kind::SavePoint);
    assert_eq!(records.len(), 2);
    assert_eq!(records[1]["payload"]["phase"], "refused");
    for row in &records {
        assert_eq!(row["payload"]["capture_id"], id.0.to_string());
        assert_eq!(
            row["payload"]["window"]["deadline_ns"],
            window.deadline_ns.0.to_string()
        );
    }
    let next = demand_window(parent.member.run);
    let received = parent.member.run.clock.now().unwrap();
    let mut retry = demand(parent.member.run, next, received);
    retry.step().unwrap();
    assert!(
        retry
            .capture
            .begin_demand_boundary(&mut retry.member.run.context, &mut retry.member.run.clock)
            .unwrap()
    );
    assert!(
        matches!(retry.next().payload, CapturePayload::SavePointBeginning(beginning) if beginning.capture_id != id)
    );
}

#[test]
fn demand_capture_image_check_expiry_aborts_without_halting_and_allows_retry() {
    expire_demand_before_commit(false);
}

#[test]
fn demand_capture_prepare_expiry_aborts_without_halting_and_allows_retry() {
    expire_demand_before_commit(true);
}

#[test]
fn demand_capture_expiry_withdraws_unsent_commit_before_refusing() {
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
    parent.submit(CapturePayload::SavePointPrepared(artifact));
    parent.pressure();
    for _ in 0..8 {
        parent.step().unwrap();
    }
    assert_eq!(
        parent.member.records(Kind::SavePoint).len(),
        2,
        "Confirmed is retained before the blocked Commit"
    );
    assert!(
        !parent
            .capture
            .at_halt(&parent.member.run.context)
            .commit_sent
    );
    parent
        .member
        .run
        .clock
        .advance_to(window.deadline_ns.0)
        .unwrap();
    let request = parent.request();
    assert!(
        matches!(request, s::StateFrame::Control { frame: s::StateControl::Abort { capture_id }, .. } if capture_id == beginning.capture_id)
    );
    let answer = parent.member.answer(
        &request,
        s::StateControlAnswer::Aborted {
            capture_id: beginning.capture_id,
        },
    );
    parent.member.receive(&answer).unwrap();
    for _ in 0..64 {
        if parent.step().unwrap() == ServingCaptureProgress::Refused {
            break;
        }
    }
    parent.drain_pressure();
    assert_eq!(parent.step().unwrap(), ServingCaptureProgress::Refused);
    assert_eq!(parent.capture.refusal_leg(), Some(SavePointLeg::Published));
    parent
        .peer
        .try_send_observed(&mut parent.member.run.clock)
        .unwrap();
    assert!(parent.member.run.context.failure().is_none());
    assert!(parent.member.run.context.capture_snapshot().is_none());
    assert!(
        !parent
            .capture
            .at_halt(&parent.member.run.context)
            .commit_sent
    );
    assert_eq!(parent.member.records(Kind::SavePoint).len(), 3);
    let mut bytes = [0; 4096];
    assert_eq!(
        recv(parent.admin.as_raw_fd(), &mut bytes, MsgFlags::MSG_DONTWAIT),
        Err(nix::errno::Errno::EAGAIN),
        "the withdrawn Commit cannot reappear after pressure drains"
    );
}

#[test]
fn demand_capture_expired_prepare_keeps_late_reply_without_granting_commit() {
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
    parent
        .member
        .run
        .clock
        .advance_to(window.deadline_ns.0)
        .unwrap();
    assert_eq!(parent.step().unwrap(), ServingCaptureProgress::Progress);
    parent.submit(CapturePayload::SavePointPrepared(artifact));
    let request = parent.request();
    assert!(
        matches!(request, s::StateFrame::Control { frame: s::StateControl::Abort { capture_id }, .. } if capture_id == beginning.capture_id)
    );
    let answer = parent.member.answer(
        &request,
        s::StateControlAnswer::Aborted {
            capture_id: beginning.capture_id,
        },
    );
    parent.member.receive(&answer).unwrap();
    for _ in 0..64 {
        if parent.step().unwrap() == ServingCaptureProgress::Refused {
            break;
        }
    }
    assert_eq!(parent.step().unwrap(), ServingCaptureProgress::Refused);
    assert!(parent.member.run.context.failure().is_none());
    assert!(
        !parent
            .capture
            .at_halt(&parent.member.run.context)
            .commit_sent
    );
    assert_eq!(parent.member.records(Kind::SavePoint).len(), 2);
}

#[test]
fn demand_capture_expiry_withdraws_commit_after_admin_disconnect() {
    let mut run = active();
    let window = demand_window(&mut run);
    let received = run.clock.now().unwrap();
    let mut parent = demand(&mut run, window, received);
    parent.step().unwrap();
    assert!(
        parent
            .capture
            .begin_demand_boundary(&mut parent.member.run.context, &mut parent.member.run.clock,)
            .unwrap()
    );
    let (beginning, artifact) = parent.prepared();
    parent.submit(CapturePayload::SavePointPrepared(artifact));
    parent.pressure();
    for _ in 0..8 {
        parent.step().unwrap();
    }
    assert!(
        !parent
            .capture
            .at_halt(&parent.member.run.context)
            .commit_sent
    );
    nix::sys::socket::shutdown(parent.admin.as_raw_fd(), nix::sys::socket::Shutdown::Both).unwrap();
    assert_eq!(
        parent
            .peer
            .try_send_observed(&mut parent.member.run.clock)
            .unwrap_err()
            .fault,
        crate::ChannelFault::Closed
    );
    parent
        .member
        .run
        .clock
        .advance_to(window.deadline_ns.0)
        .unwrap();
    let expiry = parent
        .member
        .run
        .context
        .live(&mut parent.member.run.clock)
        .unwrap_err();
    assert!(
        parent
            .capture
            .retain_window_expiry(&parent.member.run.context, &expiry)
    );
    parent
        .capture
        .prepare_retained_window_expiry(
            &mut parent.member.run.context,
            &mut parent.peer,
            &mut parent.member.run.clock,
        )
        .unwrap();
    let request = parent.request();
    assert!(
        matches!(request, s::StateFrame::Control { frame: s::StateControl::Abort { capture_id }, .. }
        if capture_id == beginning.capture_id)
    );
    let answer = parent.member.answer(
        &request,
        s::StateControlAnswer::Aborted {
            capture_id: beginning.capture_id,
        },
    );
    parent.member.receive(&answer).unwrap();
    for _ in 0..64 {
        if parent.step().unwrap() == ServingCaptureProgress::Refused {
            break;
        }
    }
    assert_eq!(parent.step().unwrap(), ServingCaptureProgress::Refused);
    assert!(parent.member.run.context.failure().is_none());
    assert!(parent.member.run.context.capture_snapshot().is_none());
    assert!(
        !parent
            .capture
            .at_halt(&parent.member.run.context)
            .commit_sent
    );
    assert_eq!(parent.member.records(Kind::SavePoint).len(), 3);
}

#[test]
fn demand_capture_expiry_withdraws_commit_before_a_cleanup_clock_failure() {
    struct BrokenClock;
    impl PumpIo for BrokenClock {
        fn now(&mut self) -> std::io::Result<u64> {
            Err(std::io::Error::other("cleanup clock failed"))
        }
        fn wait(
            &mut self,
            _: &[crate::flow::clock::WaitInterest<'_>],
            _: Option<u64>,
        ) -> std::io::Result<crate::flow::clock::WaitOutcome> {
            panic!("withdrawal must not wait");
        }
    }
    for outer_worker in [true, false] {
        let mut run = active();
        let window = demand_window(&mut run);
        let received = run.clock.now().unwrap();
        let mut parent = demand(&mut run, window, received);
        parent.step().unwrap();
        assert!(parent.capture.begin_demand_boundary(
            &mut parent.member.run.context,
            &mut parent.member.run.clock,
        ).unwrap());
        let (_, artifact) = parent.prepared();
        parent.submit(CapturePayload::SavePointPrepared(artifact));
        parent.pressure();
        for _ in 0..8 {
            parent.step().unwrap();
        }
        assert!(
            !parent
                .capture
                .at_halt(&parent.member.run.context)
                .commit_sent
        );
        parent
            .member
            .run
            .clock
            .advance_to(window.deadline_ns.0)
            .unwrap();
        let expiry = parent
            .member
            .run
            .context
            .live(&mut parent.member.run.clock)
            .unwrap_err();
        assert!(
            parent
                .capture
                .retain_window_expiry(&parent.member.run.context, &expiry)
        );
        let failure = if outer_worker {
            parent.capture.prepare_retained_window_expiry(
                &mut parent.member.run.context,
                &mut parent.peer,
                &mut BrokenClock,
            )
        } else {
            parent
                .capture
                .advance(
                    &mut parent.member.run.context,
                    &mut PumpPorts {
                        lifecycle: LifecyclePorts {
                            leave_peer: Some(&mut parent.peer),
                            ..Default::default()
                        },
                        gate: None,
                        decode: None,
                        label: None,
                        io: &mut BrokenClock,
                    },
                )
                .map(|_| ())
        }
        .unwrap_err();
        assert!(matches!(
            failure,
            RunFailure::Flow {
                reason: FlowHaltReason::TraceFailed,
                ..
            }
        ));
        parent.drain_pressure();
        parent
            .peer
            .try_send_observed(&mut parent.member.run.clock)
            .unwrap();
        let mut bytes = [0; 4096];
        assert_eq!(
            recv(parent.admin.as_raw_fd(), &mut bytes, MsgFlags::MSG_DONTWAIT),
            Err(nix::errno::Errno::EAGAIN),
            "expired Commit escaped after cleanup failed (worker={outer_worker})"
        );
        assert!(
            !parent
                .capture
                .at_halt(&parent.member.run.context)
                .commit_sent
        );
    }
}

#[test]
fn demand_capture_expiry_before_beginning_starts_no_member_capture() {
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
    parent.pressure();
    assert_eq!(parent.step().unwrap(), ServingCaptureProgress::Pending);
    parent
        .member
        .run
        .clock
        .advance_to(window.deadline_ns.0)
        .unwrap();
    for _ in 0..32 {
        if parent.step().unwrap() == ServingCaptureProgress::Refused {
            break;
        }
    }
    parent.drain_pressure();
    assert_eq!(parent.step().unwrap(), ServingCaptureProgress::Refused);
    assert_eq!(parent.capture.refusal_leg(), Some(SavePointLeg::Answer));
    assert!(parent.member.run.context.capture_snapshot().is_none());
    assert!(
        parent
            .member
            .run
            .context
            .flow
            .state()
            .outstanding()
            .is_empty()
    );
    assert!(parent.member.run.context.failure().is_none());
    let mut bytes = [0; 4096];
    assert_eq!(
        recv(parent.admin.as_raw_fd(), &mut bytes, MsgFlags::MSG_DONTWAIT),
        Err(nix::errno::Errno::EAGAIN)
    );
    assert_eq!(parent.member.records(Kind::SavePoint).len(), 2);
}

#[test]
fn demand_capture_expiry_retains_the_confirmation_author_through_cleanup() {
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
    parent.member.fill_trace();
    parent.submit(CapturePayload::SavePointPrepared(artifact));
    parent.step().unwrap();
    parent.step().unwrap();
    assert!(
        parent.member.run.clock.now().unwrap() >= window.deadline_ns.0,
        "the blocked confirmation reaches the original capture bound"
    );
    assert!(
        !parent
            .capture
            .at_halt(&parent.member.run.context)
            .commit_sent
    );
    parent.member.drain_trace();
    let request = parent.request();
    assert!(
        matches!(request, s::StateFrame::Control { frame: s::StateControl::Abort { capture_id }, .. } if capture_id == beginning.capture_id)
    );
    let answer = parent.member.answer(
        &request,
        s::StateControlAnswer::Aborted {
            capture_id: beginning.capture_id,
        },
    );
    parent.member.receive(&answer).unwrap();
    for _ in 0..64 {
        if parent.step().unwrap() == ServingCaptureProgress::Refused {
            break;
        }
    }
    assert_eq!(parent.step().unwrap(), ServingCaptureProgress::Refused);
    let rows = parent.member.records(Kind::SavePoint);
    assert_eq!(rows.len(), 3);
    assert_eq!(rows[1]["payload"]["phase"], "confirmed");
    assert_eq!(rows[2]["payload"]["phase"], "refused");
    assert!(parent.member.run.context.failure().is_none());
    assert!(parent.member.run.context.capture_snapshot().is_none());
    assert!(
        !parent
            .capture
            .at_halt(&parent.member.run.context)
            .commit_sent
    );
}

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
    use crate::flow::context::serving_observation::ServingObservationAuthor;
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
    let claim = Arc::new(ServingObservationAuthor::new(context, &payload));
    let other = Arc::new(ServingObservationAuthor::new(context, &payload));
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
        .author_serving_observation(&mut ports, &claim, original)
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
        context.serving_observation_author.as_ref().unwrap(),
        &claim
    ));
    let competitor = context.author_and_wait(&mut ports, event()).unwrap_err();
    assert_eq!(competitor.reason, RunFailure::Busy(HeldInput::Caller));
    assert!(competitor.original.is_some());
    let impostor = context
        .author_serving_observation(&mut ports, &other, event())
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
    let sequence = context
        .resume_serving_observation(&mut ports, &claim)
        .unwrap();
    assert_eq!(claim.sequence(), Some(sequence));
    assert!(other.sequence().is_none());
    assert!(context.serving_observation_author.is_none());
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

#[test]
fn queued_save_yields_its_unstarted_record_while_context_is_releasing() {
    let mut run = active();
    let window = demand_window(&mut run);
    let received = run.clock.now().unwrap();
    let mut parent = demand(&mut run, window, received);
    let original = parent.capture.at_halt(&parent.member.run.context);
    parent.member.run.context.phase = RecordingPhase::Releasing;
    for _ in 0..3 {
        assert_eq!(parent.step().unwrap(), ServingCaptureProgress::Pending);
        assert!(parent.member.records(Kind::SavePoint).is_empty());
        assert!(parent.member.run.context.failure().is_none());
        assert!(
            parent
                .member
                .run
                .context
                .serving_observation_author
                .is_none()
        );
        assert_eq!(
            parent
                .capture
                .at_halt(&parent.member.run.context)
                .capture_id,
            original.capture_id
        );
    }
    // This boundary test restores the same eligible phase to prove that the
    // original request was retained, not discarded or submitted on refusal.
    parent.member.run.context.phase = RecordingPhase::Entered;
    parent.step().unwrap();
    let rows = parent.member.records(Kind::SavePoint);
    assert_eq!(rows.len(), 1);
    assert_eq!(
        rows[0]["payload"]["capture_id"],
        original.capture_id.0.to_string()
    );
}
