//! Real Lower datagrams, actual gate exit, and the unchanged accepted Leave.
use super::*;
use crate::flow::context::failed_enter_driver::ChildState;

pub(super) fn is_mode(mode: &str) -> bool {
    mode.starts_with("expired-startup-raised-lower-")
}

pub(super) fn reached(enter: &FoundationEnter<'_>, mode: &str) -> bool {
    if mode.ends_with("-unreplied") {
        matches!(enter.state.as_ref(), Some(EnterState::Gate(gate))
            if gate.expired_test_raise_sent_without_reply())
    } else {
        matches!(enter.state.as_ref(), Some(EnterState::Ready(_)))
    }
}

struct CleanupIo<'a>(&'a mut FakePumpIo);
impl PumpIo for CleanupIo<'_> {
    fn now(&mut self) -> std::io::Result<u64> {
        self.0.now()
    }

    fn wait(
        &mut self,
        interests: &[WaitInterest<'_>],
        bound: Option<u64>,
    ) -> std::io::Result<crate::flow::clock::WaitOutcome> {
        if interests.is_empty() {
            self.0.wait(interests, bound)
        } else {
            CircuitIo(self.0).wait(interests, bound)
        }
    }
}

fn gate(owner: &FoundationExpiredRollback) -> &super::super::gate::ExpiredGateRollback {
    owner.active.as_ref().unwrap().gate.as_ref().unwrap()
}

fn child(owner: &FoundationExpiredRollback) -> ChildState {
    owner
        .handoff
        .gate
        .as_ref()
        .unwrap()
        .child
        .as_ref()
        .unwrap()
        .state()
}

fn advance(
    owner: &mut FoundationExpiredRollback,
    clock: &mut FakePumpIo,
    until: Instant,
) -> FoundationExpiredRollbackProgress {
    assert!(
        Instant::now() < until,
        "gate rollback watchdog: {:?}",
        owner.detail()
    );
    let progress = owner.advance_with_io(&mut CleanupIo(clock));
    assert_ne!(
        progress,
        FoundationExpiredRollbackProgress::Stopped,
        "{:?}",
        owner.detail()
    );
    assert!(owner.run.context.failure().is_none());
    assert!(owner.run.context.flow.halt().is_none());
    progress
}

fn wait_file(path: &Path, until: Instant) {
    while !path.exists() {
        assert!(
            Instant::now() < until,
            "gate child did not create {}",
            path.display()
        );
        std::thread::sleep(Duration::from_millis(1));
    }
}

#[allow(clippy::too_many_arguments)]
pub(super) fn finish(
    owner: &mut FoundationExpiredRollback,
    member: &mut Member,
    reader: &mut UnixStream,
    clock: &mut FakePumpIo,
    path: &Path,
    peer: &OwnedFd,
    directive: &OrganEnvelope,
    requests: &[String],
    mode: &str,
) {
    let until = Instant::now() + Duration::from_secs(12);
    let root = path.parent().unwrap();
    let leave = owner.leave.unwrap();
    let enter = owner.handoff.identity.enter;
    let unreplied = mode.ends_with("-unreplied");
    let early = mode.ends_with("-early");
    let silent = mode.ends_with("-silent") || mode.ends_with("-silent-admin");
    let held = mode.ends_with("-held");
    let cap = mode.ends_with("-cap");
    let pressure = mode.ends_with("-pressure");
    let transport_loss = mode.ends_with("-transport-loss");
    let original_gate = match owner.handoff.held.as_ref() {
        HeldStartup::Gate(gate) | HeldStartup::Ready { gate, .. } => gate,
        _ => panic!("actual expired gate account"),
    };
    let raise = original_gate.raise.clone();
    assert!(original_gate.send.as_ref().is_some_and(|send| {
        send.sent()
            && send
                .sent_at()
                .is_some_and(|at| at >= enter.began_ns() && !enter.expired(at))
    }));
    assert_eq!(original_gate.reply.is_none(), unreplied);
    if early {
        wait_file(&root.join("gate-early-sent"), until);
    }
    if mode.ends_with("-frames") || pressure {
        wait_file(&root.join("gate-frames-sent"), until);
    }
    // Construction is deliberately later than acceptance. A new bound based
    // on this clock would incorrectly extend the original Lower allowance.
    assert!(
        owner
            .active
            .as_ref()
            .is_none_or(|active| active.gate.is_none())
    );
    clock.advance_to(leave.began_ns() + 1_000_000).unwrap();
    assert!(clock.now().unwrap() > leave.began_ns());
    // The active owner and original Enter retirement are reached by normal
    // advancement. No synthetic deadline or sent proof stands in for them.
    while owner
        .active
        .as_ref()
        .is_none_or(|active| active.gate.is_none())
    {
        assert_eq!(
            advance(owner, clock, until),
            FoundationExpiredRollbackProgress::Accepted
        );
        assert!(packet(peer).is_none());
        member.assert_no_request();
    }
    let lower = gate(owner).bound();
    assert_eq!(lower.began_ns(), leave.began_ns());
    assert_eq!(
        lower.deadline_ns(),
        leave.began_ns() + weaver_types::LOWER_BOUND_MS * 1_000_000
    );
    assert_eq!(owner.run.context.retired_enter, Some(enter));
    assert!(
        owner
            .run
            .context
            .deadlines
            .get(PumpDeadline::Enter)
            .is_none()
    );
    assert_eq!(
        owner.run.context.deadlines.get(PumpDeadline::Leave),
        Some(leave)
    );

    let mut trace_reader = None;
    let mut pending_refusal = None;
    if pressure {
        // The first real Frame is owned before its author is submitted. The
        // Lower has already been sent, despite the incoming content packet.
        let raw = gate(owner)
            .test_pending_frame()
            .expect("actual queued gate frame");
        let original = raw.to_vec();
        let address = raw.as_ptr();
        assert!(gate(owner).test_lower().unwrap().1);
        wait_file(&root.join("gate-lower-received"), until);
        super::super::super::failed::tests::fill_trace(&mut owner.run.context, clock);
        // No scheduled readiness: the common author wait reaches the actual
        // original Lower deadline with a real capacity-pending trace ticket.
        assert_eq!(
            owner.advance_with_io(clock),
            FoundationExpiredRollbackProgress::Accepted,
            "{:?}",
            owner.detail()
        );
        assert_eq!(clock.now().unwrap(), lower.deadline_ns());
        assert!(owner.run.context.failure().is_none());
        assert!(owner.run.context.flow.halt().is_none());
        assert!(gate(owner).swept());
        assert!(gate(owner).test_retired());
        assert!(
            owner
                .run
                .context
                .deadlines
                .get(PumpDeadline::Lower)
                .is_none()
        );
        assert_eq!(
            owner.run.context.deadlines.get(PumpDeadline::Leave),
            Some(leave)
        );
        assert_eq!(gate(owner).test_pending_frame().unwrap(), original);
        assert_eq!(gate(owner).test_pending_frame().unwrap().as_ptr(), address);
        let ticket = owner.run.context.barrier.pending_author().unwrap().ticket;
        let event = owner
            .run
            .context
            .barrier
            .pending()
            .expect("actual pending refusal");
        assert_eq!(event.envelope.kind, weaver_trace::serving::Kind::Refusal);
        pending_refusal = Some(serde_json::to_value(event).unwrap());
        assert!(owner.unload_sequence().is_none());
        assert!(packet(peer).is_none());
        crate::flow::context::rollback_gate_author_tests::competing_author(
            &mut owner.run.context,
            clock,
        );

        // Only now drain the real trace sink. The same refused Frame resumes
        // under the original Leave; Lower expiry grants no replacement ticket.
        use std::io::BufRead;
        let input = reader.try_clone().unwrap();
        input
            .set_read_timeout(Some(Duration::from_secs(8)))
            .unwrap();
        trace_reader = Some(std::thread::spawn(move || {
            let mut rows = vec![];
            for line in std::io::BufReader::new(input).lines() {
                let row: serde_json::Value = serde_json::from_str(&line.unwrap()).unwrap();
                let terminal = row["kind"] == "unload";
                rows.push(row);
                if terminal {
                    return rows;
                }
            }
            panic!("gate pressure trace ended before Unload");
        }));
        assert_eq!(
            owner.run.context.barrier.pending_author().unwrap().ticket,
            ticket
        );
        assert_eq!(gate(owner).test_pending_frame().unwrap().as_ptr(), address);
    }

    let mut secondary = None;
    if mode.ends_with("-silent-admin") {
        // Accept a real second root dial without sending a request. Observe
        // its original read deadline before jumping across both deadlines.
        secondary = Some(dial(path));
        while owner
            .run
            .context
            .deadlines
            .get(PumpDeadline::AdminRead)
            .is_none()
        {
            assert_eq!(
                advance(owner, clock, until),
                FoundationExpiredRollbackProgress::Accepted
            );
            assert!(packet(peer).is_none());
        }
        let read = owner
            .run
            .context
            .deadlines
            .get(PumpDeadline::AdminRead)
            .unwrap();
        assert!(read.deadline_ns() < lower.deadline_ns());
    }
    if silent || held || cap {
        while !root.join("gate-lower-received").exists() {
            assert_eq!(
                advance(owner, clock, until),
                FoundationExpiredRollbackProgress::Accepted
            );
            member.assert_no_request();
            assert!(packet(peer).is_none());
            std::thread::sleep(Duration::from_millis(1));
        }
        if held {
            wait_file(&root.join("gate-stopped-sent"), until);
            while gate(owner).test_replies().1.is_none() {
                assert_eq!(
                    advance(owner, clock, until),
                    FoundationExpiredRollbackProgress::Accepted
                );
            }
            for _ in 0..8 {
                assert_eq!(
                    advance(owner, clock, until),
                    FoundationExpiredRollbackProgress::Accepted
                );
                assert_eq!(child(owner), ChildState::Running);
                assert!(gate(owner).swept());
                assert!(!gate(owner).test_disposition().1);
                assert!(owner.unload_sequence().is_none());
                assert!(owner.active.as_ref().unwrap().release.is_none());
                assert_eq!(member.requests, requests);
                member.assert_no_request();
                assert!(
                    packet(peer).is_none(),
                    "GateStopped is not an actual child exit"
                );
            }
            std::fs::write(root.join("gate-exit"), b"exit").unwrap();
        }
        if silent {
            if secondary.is_none() {
                clock.advance_to(lower.deadline_ns() - 1_000_000).unwrap();
                assert_eq!(
                    advance(owner, clock, until),
                    FoundationExpiredRollbackProgress::Accepted
                );
            } else {
                assert!(
                    owner
                        .run
                        .context
                        .deadlines
                        .get(PumpDeadline::AdminRead)
                        .is_some()
                );
            }
            assert!(
                !gate(owner).swept(),
                "the original Lower bound has not elapsed"
            );
            assert!(owner.handoff.gate.as_ref().unwrap().port.is_some());
            clock.advance_to(lower.deadline_ns()).unwrap();
            assert_eq!(
                advance(owner, clock, until),
                FoundationExpiredRollbackProgress::Accepted
            );
            assert!(gate(owner).swept());
            assert!(owner.handoff.gate.as_ref().unwrap().port.is_none());
            assert!(
                owner
                    .run
                    .context
                    .deadlines
                    .get(PumpDeadline::Lower)
                    .is_none()
            );
            assert!(
                owner
                    .run
                    .context
                    .deadlines
                    .get(PumpDeadline::AdminRead)
                    .is_none()
            );
            assert_eq!(
                owner.run.context.deadlines.get(PumpDeadline::Leave),
                Some(leave)
            );
        }
        if cap {
            clock.advance_to(leave.deadline_ns()).unwrap();
            assert_eq!(
                advance(owner, clock, until),
                FoundationExpiredRollbackProgress::Closed
            );
            assert!(owner.abandoned());
            assert!(gate(owner).swept());
            assert!(owner.handoff.gate.as_ref().unwrap().port.is_none());
            assert!(owner.unload_sequence().is_none());
            assert!(
                packet(peer).is_none(),
                "cap disposal does not manufacture Left"
            );
            let evidence = gate(owner).test_disposition();
            assert_eq!(
                advance(owner, clock, until),
                FoundationExpiredRollbackProgress::Closed
            );
            assert_eq!(gate(owner).test_disposition(), evidence);
            assert_eq!(member.requests, requests);
            return;
        }
    }

    loop {
        let progress = advance(owner, clock, until);
        if progress == FoundationExpiredRollbackProgress::Closed {
            break;
        }
        if !gate(owner).test_disposition().1 {
            assert_eq!(member.requests, requests, "Grants preceded gate exit");
            member.assert_no_request();
            assert!(owner.active.as_ref().unwrap().release.is_none());
            assert!(owner.unload_sequence().is_none());
        } else if transport_loss {
            assert!(matches!(child(owner), ChildState::Reaped(_)));
            assert!(
                gate(owner).test_recorded().is_some(),
                "death fault before Grants"
            );
            assert_eq!(
                owner
                    .run
                    .context
                    .barrier
                    .recorder()
                    .structure()
                    .by_kind(weaver_trace::serving::Kind::Fault)
                    .count(),
                1
            );
        }
        member.step(clock.now().unwrap(), "rollback-ready");
        assert!(packet(peer).is_none(), "Left preceded terminal drain");
        std::thread::sleep(Duration::from_millis(1));
    }
    assert!(!owner.abandoned());
    assert!(gate(owner).swept());
    assert!(gate(owner).test_disposition().1);
    assert!(matches!(child(owner), ChildState::Reaped(_)));
    if held || transport_loss {
        assert!(
            matches!(
                child(owner),
                ChildState::Reaped(nix::sys::wait::WaitStatus::Exited(_, 0))
            ),
            "the actual child must complete every Lower/duplicate-send assertion"
        );
    }
    assert!(gate(owner).test_retired());
    assert!(
        owner
            .run
            .context
            .deadlines
            .get(PumpDeadline::Lower)
            .is_none()
    );
    assert_eq!(
        owner.run.context.deadlines.get(PumpDeadline::Leave),
        Some(leave)
    );
    let (request, sent, sent_at) = gate(owner).test_lower().unwrap();
    assert_eq!(request.exchange.opener, Opener::Harness);
    assert_eq!(request.exchange.ordinal, raise.exchange.ordinal + 1);
    assert_eq!(
        request.payload,
        Payload::Directive(LifecycleDirective::Lower)
    );
    assert_eq!(sent, !early);
    if let Some(at) = sent_at {
        assert!(at >= leave.began_ns() && !lower.expired(at));
    }
    if early {
        assert!(
            !root.join("gate-lower-received").exists(),
            "early reply cannot become a sent Lower"
        );
    } else {
        let bytes = std::fs::read(root.join("gate-lower-received")).unwrap();
        assert_eq!(
            serde_json::from_slice::<OrganEnvelope>(&bytes).unwrap(),
            *request
        );
    }
    let (late_raise, lower_reply) = gate(owner).test_replies();
    if unreplied {
        let reply = late_raise.expect("retain the actual late Raise reply");
        assert_eq!(reply.envelope().exchange, raise.exchange);
        assert!(matches!(
            reply.envelope().payload,
            Payload::Answer(LifecycleAnswer::GateReady)
        ));
    } else {
        assert!(late_raise.is_none());
    }
    assert_eq!(lower_reply.is_none(), silent || pressure || transport_loss);
    let expected_refusal = if early || mode.ends_with("-wrong") {
        Some(LifecycleRefusal::Malformed)
    } else if mode.ends_with("-refused") {
        Some(LifecycleRefusal::OrganRefused {
            organ: weaver_types::RefusingOrgan::Gate,
            reason: Box::new(LifecycleRefusal::OutOfOrder),
        })
    } else {
        None
    };
    assert_eq!(gate(owner).refusal(), expected_refusal.as_ref());
    let answer = packet(peer).expect("actual terminal Leave answer");
    assert_eq!(answer.exchange, directive.exchange);
    assert_eq!(answer.position, Position::Close);
    if let Some(refusal) = expected_refusal {
        assert_eq!(answer.payload, Payload::Refusal(refusal));
    } else {
        assert!(matches!(
            answer.payload,
            Payload::Answer(LifecycleAnswer::Left {
                save_point: None,
                forced: true,
                no_state: false,
            })
        ));
    }
    let mut expected = requests.to_vec();
    expected.push("grants".into());
    assert_eq!(member.requests, expected);
    member.assert_closed_without_request();
    let rows = if let Some(trace_reader) = trace_reader {
        trace_reader.join().unwrap()
    } else {
        reader.set_nonblocking(true).unwrap();
        super::super::super::tests::trace_through(
            reader,
            vec![],
            weaver_trace::Sequence(owner.unload_sequence().unwrap()),
            until,
        )
        .unwrap()
    };
    let unloading = serde_json::to_value(weaver_types::RefusalRecord::Lifecycle {
        asked: weaver_types::LifecycleAsk::Leave,
        refusal: LifecycleRefusal::Unloading,
    })
    .unwrap();
    if transport_loss {
        let faults: Vec<_> = rows.iter().filter(|row| row["kind"] == "fault").collect();
        assert_eq!(faults.len(), 1, "actual gate EOF owes one death fault");
        let report: weaver_types::FaultReport =
            serde_json::from_value(faults[0]["payload"].clone()).unwrap();
        assert_eq!(report.case, weaver_types::FaultCase::OrganDeathObserved);
        let account: serde_json::Value = serde_json::from_str(report.account.get()).unwrap();
        assert_eq!(account["organ"], "gate");
        assert_eq!(account["death"], "its channel was lost at the lower");
        assert!(
            account.get("last_word").is_none(),
            "untyped stderr supplies no last_word"
        );
        assert!(faults[0].get("turn").is_none_or(serde_json::Value::is_null));
        assert_eq!(
            faults[0]["sequence"],
            serde_json::to_value(gate(owner).test_recorded().unwrap()).unwrap()
        );
    }
    let refusals: Vec<_> = rows
        .iter()
        .filter(|row| row["kind"] == "refusal" && (!pressure || row["payload"] == unloading))
        .collect();
    assert_eq!(
        refusals.len(),
        if mode.ends_with("-frames") {
            3
        } else {
            usize::from(pressure)
        }
    );
    if let Some(mut pending) = pending_refusal {
        // Preparing an author does not allocate its eventual trace sequence.
        // Use this exact driver's retained Recorded proof, not the row itself,
        // to finish the expected envelope after the original ticket settles.
        let recorded = gate(owner)
            .test_recorded()
            .expect("the owed refusal Recorded");
        let sequence = serde_json::to_value(recorded).unwrap();
        assert_eq!(refusals[0]["sequence"], sequence);
        pending["sequence"] = sequence;
        assert_eq!(
            *refusals[0], pending,
            "the actual pending record reached the sink unchanged"
        );
    }
    for row in refusals {
        assert_eq!(row["payload"], unloading);
        assert!(row.get("turn").is_none_or(serde_json::Value::is_null));
    }
    assert!(
        rows.iter()
            .all(|row| !matches!(row["kind"].as_str(), Some("turn.started" | "save_point")))
    );
    assert_eq!(rows.iter().filter(|row| row["kind"] == "unload").count(), 1);
    let unload = rows.last().unwrap();
    assert_eq!(unload["kind"], "unload");
    assert_eq!(unload["payload"]["forced"], true);
    assert_eq!(unload["payload"]["release"]["spu"], "confirmed");
    assert_eq!(unload["payload"]["release"]["member"], "closed");
    assert_eq!(
        advance(owner, clock, until),
        FoundationExpiredRollbackProgress::Closed
    );
    assert!(packet(peer).is_none(), "terminal reply is sent only once");
    drop(secondary);
}
