//! Real state failures, pending recorder work and the original halt deadline.
use super::*;
use crate::flow::halt_snapshot::ExternalFacts;
use std::time::{Duration, Instant};

fn freeze(context: &mut RecordingContext) {
    context
        .freeze_halt_account(ExternalFacts {
            outstanding_pairs: context.outstanding_pairs(),
            capture: None,
            wait: context.halt_wait().cloned(),
            input_id: context.input.snapshot().map(|input| input.input_id),
        })
        .unwrap();
}
fn begin_report(member: &mut Member) {
    freeze(&mut member.run.context);
    member.run.context.begin_halt_report().unwrap();
}
fn advance(member: &mut Member) -> HaltTraceProgress {
    member
        .run
        .context
        .advance_halt_trace(&mut member.run.clock)
        .unwrap()
}
fn bound_failure(member: &mut Member) -> RunFailure {
    let bound = AbsoluteBound::after(member.now(), 2).unwrap();
    member
        .run
        .context
        .arm(PumpDeadline::Gate, bound, &mut member.run.clock)
        .unwrap();
    member.run.clock.advance_to(bound.deadline_ns()).unwrap();
    let failure = member
        .run
        .context
        .reduce_flow(&mut member.run.clock)
        .unwrap_err();
    assert!(matches!(
        failure,
        RunFailure::Flow {
            reason: FlowHaltReason::GateBound,
            ..
        }
    ));
    failure
}
fn pending(member: &mut Member) -> (u64, String) {
    member.fill_trace();
    let now = member.now();
    let payload =
        serde_json::value::to_raw_value(&serde_json::json!({"owed":"exact report fixture"}))
            .unwrap();
    let context = &mut member.run.context;
    assert!(matches!(
        context.barrier.submit_authored_with_clock(
            AuthoredEvent {
                kind: Kind::ModelMeasurement,
                subsystem: Subsystem::Harness,
                turn: Some(TurnRef("pressure".into())),
                source: super::super::super::author::AuthoredSource::Runtime,
                causal_parent: None,
                payload: Some(Payload::ModelMeasurement(payload))
            },
            RecordPurpose::Observation,
            &context.author,
            1,
            &mut context.inbox,
            &mut context.budget,
            &mut || Ok(now),
        ),
        RecordProgress::Pending
    ));
    let ticket = context.pending_author().unwrap();
    (ticket.ticket, format!("{:?}", ticket.event))
}

#[test]
fn halt_report_requires_failed_context_and_real_frozen_account_once() {
    let mut member = Member::new();
    assert_eq!(
        member.run.context.begin_halt_report(),
        Err(HaltReportRefusal::NotFailed)
    );
    bound_failure(&mut member);
    assert_eq!(
        member.run.context.advance_halt_trace(&mut member.run.clock),
        Err(HaltReportRefusal::NotStarted)
    );
    assert_eq!(
        member.run.context.begin_halt_report(),
        Err(HaltReportRefusal::MissingAccount)
    );
    begin_report(&mut member);
    assert_eq!(
        member.run.context.begin_halt_report(),
        Err(HaltReportRefusal::AlreadyStarted)
    );
    assert!(member.records(Kind::Fault).is_empty());
}

#[test]
fn halt_report_fault_keeps_the_actual_recorded_open_turn() {
    let mut member = Member::new();
    let mut started = super::super::tests::event(Kind::TurnStarted, None);
    started.turn = Some(TurnRef("halted-turn".into()));
    let sequence = member.run.author(started).unwrap();
    let first = bound_failure(&mut member);
    begin_report(&mut member);
    assert!(matches!(
        advance(&mut member),
        HaltTraceProgress::FaultRecorded(_)
    ));
    let starts = member.records(Kind::TurnStarted);
    let faults = member.records(Kind::Fault);
    assert_eq!(starts.len(), 1);
    assert_eq!(starts[0]["sequence"], sequence.0.to_string());
    assert_eq!(starts[0]["turn"], "halted-turn");
    assert_eq!(faults.len(), 1);
    assert_eq!(faults[0]["turn"], starts[0]["turn"]);
    assert_eq!(faults[0]["run"], starts[0]["run"]);
    assert_eq!(member.run.context.failure(), Some(&first));
    assert!(member.records(Kind::TurnClosed).is_empty());
}

#[test]
fn halt_close_promotes_the_exact_pending_terminal_without_ordinary_capacity() {
    for reserved in [true, false] {
        let mut member = Member::new();
        let mut started = super::super::tests::event(Kind::TurnStarted, None);
        started.turn = Some(TurnRef("pending-halt-close".into()));
        member.run.author(started).unwrap();
        member.fill_trace();
        let context = &mut member.run.context;
        let event = AuthoredEvent {
            kind: Kind::TurnClosed,
            subsystem: Subsystem::Harness,
            turn: Some(TurnRef("pending-halt-close".into())),
            source: crate::flow::author::AuthoredSource::Runtime,
            causal_parent: None,
            payload: Some(Payload::TurnClosed(weaver_trace::TurnClose::Stopped {
                reason: weaver_trace::StopReason::Directive,
                cause: Some(weaver_trace::Cause { uid: 42 }),
            })),
        };
        context.author_turn = Some((event.kind, event.turn.clone()));
        assert!(matches!(
            context.barrier.submit_authored_with_clock(
                event,
                RecordPurpose::Observation,
                &context.author,
                1,
                &mut context.inbox,
                &mut context.budget,
                &mut || member.run.clock.now()
            ),
            RecordProgress::Pending
        ));
        let ticket = context.pending_author().unwrap().ticket;
        let first = bound_failure(&mut member);
        begin_report(&mut member);
        assert!(matches!(
            advance(&mut member),
            HaltTraceProgress::FaultRecorded(_)
        ));
        assert!(member.run.context.open_turn.is_some());
        let sequence = if reserved {
            member
                .run
                .context
                .resume_halt_terminal_and_bind()
                .unwrap()
                .expect("reserved close despite ordinary pressure")
        } else {
            assert!(
                member
                    .run
                    .context
                    .resume_trace_only_and_bind()
                    .unwrap()
                    .is_none()
            );
            super::recall_tests::drain_admitted(&mut member);
            member
                .run
                .context
                .resume_trace_only_and_bind()
                .unwrap()
                .unwrap()
        };
        assert_eq!(
            member.run.context.barrier.recorded_author_ticket(sequence),
            Some(ticket)
        );
        assert!(member.run.context.open_turn.is_none());
        assert!(member.run.context.author_turn.is_none());
        assert!(member.run.context.pending_author().is_none());
        assert!(
            member
                .run
                .context
                .resume_halt_terminal_and_bind()
                .unwrap()
                .is_none()
        );
        if reserved {
            super::recall_tests::drain_admitted(&mut member);
        } else {
            member.run._reader.set_nonblocking(false).unwrap();
            member
                .run
                ._reader
                .set_read_timeout(Some(Duration::from_secs(10)))
                .unwrap();
            let mut line = String::new();
            std::io::BufRead::read_line(
                &mut std::io::BufReader::new(&mut member.run._reader),
                &mut line,
            )
            .unwrap();
            assert_eq!(
                serde_json::from_str::<serde_json::Value>(&line).unwrap()["sequence"],
                sequence.0.to_string()
            );
        }
        let rows = member.records(Kind::TurnClosed);
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0]["sequence"], sequence.0.to_string());
        assert_eq!(rows[0]["payload"]["reason"], "directive");
        assert_eq!(rows[0]["payload"]["cause"]["uid"], 42);
        assert_eq!(member.run.context.failure(), Some(&first));
        member.no_request();
    }
}

#[test]
fn halt_close_never_spends_a_terminal_reserve_on_an_ordinary_pending_record() {
    let mut member = Member::new();
    let (ticket, event) = pending(&mut member);
    bound_failure(&mut member);
    begin_report(&mut member);
    assert!(matches!(
        advance(&mut member),
        HaltTraceProgress::FaultRecorded(_)
    ));
    assert!(member.run.context.resume_halt_terminal_and_bind().is_err());
    assert_eq!(member.run.context.pending_author().unwrap().ticket, ticket);
    assert_eq!(
        format!("{:?}", member.run.context.pending_author().unwrap().event),
        event
    );
}

#[test]
fn halt_report_real_state_failures_record_exact_account_and_never_resume_or_requery() {
    for malformed in [false, true] {
        let mut member = Member::new();
        let request = member.begin();
        let failure = if malformed {
            let answer = member.answer(&request, s::StateControlAnswer::Shape(vec![]));
            match member.receive(&answer) {
                Err(failure) => failure,
                Ok(()) => {
                    let mut failure = None;
                    for _ in 0..128 {
                        if let Err(error) = member.advance() {
                            failure = Some(error);
                            break;
                        }
                    }
                    failure.expect("wrong control variant must fail")
                }
            }
        } else {
            member.run.state.shutdown(Shutdown::Write).unwrap();
            let now = member.now() + 1;
            member.pump_at(now, PollFlags::POLLHUP).unwrap_err()
        };
        assert!(matches!(
            failure,
            RunFailure::Flow {
                reason: FlowHaltReason::CarryFailed | FlowHaltReason::ProtocolFailed,
                ..
            }
        ));
        begin_report(&mut member);
        let account = member.run.context.halt().unwrap().fault_account().unwrap();
        let expected: serde_json::Value = serde_json::from_str(account.get()).unwrap();
        assert_eq!(
            expected["request"]["id"],
            request.common().request_id.0.to_string()
        );
        let HaltTraceProgress::FaultRecorded(sequence) = advance(&mut member) else {
            panic!("fault must record");
        };
        let records = member.records(Kind::Fault);
        assert_eq!(records.len(), 1);
        assert_eq!(records[0]["payload"]["case"], "state_flow_failed");
        assert_eq!(records[0]["payload"]["account"], expected);
        assert_eq!(records[0]["sequence"], sequence.0.to_string());
        assert_eq!(
            member.run.context.halt_report().unwrap().fault_sequence(),
            Some(sequence)
        );
        for _ in 0..4 {
            assert_eq!(advance(&mut member), HaltTraceProgress::Idle);
        }
        assert_eq!(member.records(Kind::Fault).len(), 1);
        assert_eq!(
            member
                .run
                .context
                .reduce_flow(&mut member.run.clock)
                .unwrap_err(),
            failure
        );
        assert_eq!(member.run.context.phase(), RecordingPhase::Failed);
        assert_eq!(member.requests.len(), 1);
        assert!(member.run.context.read.is_some());
    }
}

#[test]
fn halt_report_uses_fault_reserve_without_stealing_pending_author_then_drains_trace_only() {
    let mut member = Member::new();
    let (ticket, event) = pending(&mut member);
    let first = bound_failure(&mut member);
    begin_report(&mut member);
    let account = member
        .run
        .context
        .halt()
        .unwrap()
        .fault_account()
        .unwrap()
        .get()
        .to_owned();
    let HaltTraceProgress::FaultRecorded(sequence) = advance(&mut member) else {
        panic!("reserved fault");
    };
    assert_eq!(member.run.context.pending_author().unwrap().ticket, ticket);
    assert_eq!(
        format!("{:?}", member.run.context.pending_author().unwrap().event),
        event
    );
    for _ in 0..4 {
        assert_eq!(advance(&mut member), HaltTraceProgress::Pending);
    }
    assert_eq!(member.records(Kind::Fault).len(), 1);
    assert_eq!(
        member
            .run
            .context
            .halt()
            .unwrap()
            .fault_account()
            .unwrap()
            .get(),
        account
    );
    let fault = &member.records(Kind::Fault)[0];
    assert_eq!(
        fault["payload"]["account"]["pending_author"]["ticket"],
        ticket.to_string()
    );
    assert!(
        fault["payload"]["account"]["pending_author"]["event"]
            .get("sequence")
            .is_none()
    );
    super::recall_tests::drain_admitted(&mut member);
    let HaltTraceProgress::TraceRecorded(owed) = advance(&mut member) else {
        panic!("owed trace");
    };
    assert!(owed.0 > sequence.0);
    assert_eq!(
        member
            .run
            .context
            .halt_report()
            .unwrap()
            .last_trace_sequence(),
        Some(owed)
    );
    assert!(member.run.context.pending_author().is_none());
    assert_eq!(advance(&mut member), HaltTraceProgress::Idle);
    assert_eq!(member.run.context.flow.ledger().facts().count(), 0);
    assert_eq!(member.run.context.inbox.queued().count(), 0);
    assert_eq!(member.run.context.budget.gauges(), (0, 0));
    assert!(member.run.context.barrier.recorder().tap_stopped());
    assert_eq!(member.run.context.failure(), Some(&first));
    member.no_request();
}

#[test]
fn halt_report_cannot_restart_or_outwait_the_earlier_leave_deadline() {
    let mut member = Member::new();
    let leave = AbsoluteBound::after(member.now(), 10).unwrap();
    member
        .run
        .context
        .arm(PumpDeadline::Leave, leave, &mut member.run.clock)
        .unwrap();
    let first = bound_failure(&mut member);
    begin_report(&mut member);
    assert_eq!(
        member.run.context.halt().unwrap().deadline().unwrap(),
        leave.deadline_ns()
    );
    member.run.clock.advance_to(leave.deadline_ns()).unwrap();
    assert_eq!(advance(&mut member), HaltTraceProgress::AbortRequired);
    assert!(
        member
            .run
            .context
            .halt_report()
            .unwrap()
            .fault_sequence()
            .is_none()
    );
    assert!(
        member
            .run
            .context
            .halt_report()
            .unwrap()
            .aborted()
            .is_some()
    );
    let secondary = member.run.context.halt().unwrap().secondary().len();
    for _ in 0..4 {
        assert_eq!(advance(&mut member), HaltTraceProgress::AbortRequired);
    }
    assert_eq!(
        member.run.context.halt().unwrap().secondary().len(),
        secondary
    );
    assert_eq!(
        member.run.context.halt().unwrap().deadline().unwrap(),
        leave.deadline_ns()
    );
    assert_eq!(member.run.context.failure(), Some(&first));
    assert!(member.records(Kind::Fault).is_empty());
}

struct Clock {
    first: u64,
    later: Option<u64>,
    calls: usize,
}
impl PumpIo for Clock {
    fn now(&mut self) -> std::io::Result<u64> {
        self.calls += 1;
        if self.calls == 1 {
            return Ok(self.first);
        }
        self.later
            .ok_or_else(|| std::io::Error::other("clock fixture failed"))
    }
    fn wait(&mut self, _: &[WaitInterest<'_>], _: Option<u64>) -> std::io::Result<WaitOutcome> {
        panic!("halt reporting never waits ahead of containment");
    }
}

#[test]
fn halt_report_post_submission_clock_failure_keeps_recorded_sequence_and_first_cause() {
    for mode in 0..3 {
        let mut member = Member::new();
        let first = bound_failure(&mut member);
        begin_report(&mut member);
        let now = member.now();
        let deadline = member.run.context.halt().unwrap().deadline().unwrap();
        let later = match mode {
            0 => None,
            1 => Some(now - 1),
            _ => Some(deadline),
        };
        let mut clock = Clock {
            first: now,
            later,
            calls: 0,
        };
        assert_eq!(
            member.run.context.advance_halt_trace(&mut clock).unwrap(),
            HaltTraceProgress::AbortRequired
        );
        let evidence = member.run.context.halt_report().unwrap();
        let sequence = evidence
            .fault_sequence()
            .expect("recorded before clock failure");
        assert!(evidence.event().is_some());
        assert!(evidence.aborted().is_some());
        assert_eq!(
            member.records(Kind::Fault)[0]["sequence"],
            sequence.0.to_string()
        );
        assert_eq!(member.run.context.failure(), Some(&first));
        assert_eq!(advance(&mut member), HaltTraceProgress::AbortRequired);
        assert_eq!(member.records(Kind::Fault).len(), 1);
    }
}

#[test]
fn halt_report_trace_only_transfer_is_retained_before_post_work_expiry() {
    let mut member = Member::new();
    pending(&mut member);
    bound_failure(&mut member);
    begin_report(&mut member);
    assert!(matches!(
        advance(&mut member),
        HaltTraceProgress::FaultRecorded(_)
    ));
    super::recall_tests::drain_admitted(&mut member);
    let now = member.now();
    let deadline = member.run.context.halt().unwrap().deadline().unwrap();
    let mut clock = Clock {
        first: now,
        later: Some(deadline),
        calls: 0,
    };
    assert_eq!(
        member.run.context.advance_halt_trace(&mut clock).unwrap(),
        HaltTraceProgress::AbortRequired
    );
    assert!(
        member
            .run
            .context
            .halt_report()
            .unwrap()
            .last_trace_sequence()
            .is_some()
    );
    assert!(member.run.context.pending_author().is_none());
    assert_eq!(member.run.context.inbox.queued().count(), 0);
}

#[test]
fn halt_report_used_fault_permit_fails_once_with_sound_writer_and_retains_original() {
    let mut member = Member::new();
    bound_failure(&mut member);
    begin_report(&mut member);
    // Exercise real lower-level permit misuse, not a fabricated writer failure.
    let original = member.run.context.halt().unwrap().fault_account().unwrap();
    let payload = serde_json::value::to_raw_value(&weaver_types::FaultReport {
        case: weaver_types::FaultCase::StateFlowFailed,
        account: original,
    })
    .unwrap();
    let event = super::super::tests::event(Kind::Fault, Some(Payload::Fault(payload)));
    let now = member.now();
    member
        .run
        .context
        .barrier
        .submit_terminal_authored(event, &member.run.context.author, now, 1)
        .unwrap();
    assert_eq!(advance(&mut member), HaltTraceProgress::Unavailable);
    let evidence = member.run.context.halt_report().unwrap();
    assert!(evidence.fault_sequence().is_none());
    assert!(evidence.event().is_some());
    assert!(evidence.unavailable().is_some());
    assert!(!evidence.trace_unavailable());
    assert!(member.run.context.barrier.standing_failure().is_none());
    let halt = member.run.context.halt().unwrap();
    assert_eq!(halt.secondary().len(), 1);
    let retained = &halt.secondary()[0];
    assert_eq!(retained.reason, FlowHaltReason::TraceFailed);
    assert_eq!(retained.point, HaltPoint::Teardown);
    assert_eq!(retained.detail, evidence.unavailable().unwrap());
    assert_eq!(
        retained.observed_ns,
        Some(member.run.context.last_ns.into())
    );
    let frozen: serde_json::Value =
        serde_json::from_str(halt.fault_account().unwrap().get()).unwrap();
    let unload: serde_json::Value =
        serde_json::from_str(halt.unload_account().unwrap().get()).unwrap();
    assert!(frozen["secondary"].as_array().unwrap().is_empty());
    assert_eq!(unload["secondary"][0]["detail"], retained.detail);
    let secondary = member.run.context.halt().unwrap().secondary().len();
    for _ in 0..4 {
        assert_eq!(advance(&mut member), HaltTraceProgress::Idle);
    }
    assert_eq!(
        member.run.context.halt().unwrap().secondary().len(),
        secondary
    );
    assert_eq!(member.records(Kind::Fault).len(), 1);
}

#[test]
fn halt_report_real_failed_sink_uses_stream_write_case_and_never_retries_reporting() {
    let mut member = Member::new();
    pending(&mut member);
    member.run._reader.shutdown(Shutdown::Both).unwrap();
    let watch = Instant::now() + Duration::from_secs(5);
    loop {
        let context = &mut member.run.context;
        if context.barrier.drain_status(&mut context.budget).is_err() {
            break;
        }
        assert!(
            Instant::now() < watch,
            "writer did not detect the closed sink"
        );
        std::thread::yield_now();
    }
    let first = member
        .run
        .context
        .reduce_flow(&mut member.run.clock)
        .unwrap_err();
    assert!(matches!(
        first,
        RunFailure::Flow {
            reason: FlowHaltReason::TraceFailed,
            ..
        }
    ));
    begin_report(&mut member);
    assert_eq!(advance(&mut member), HaltTraceProgress::Unavailable);
    let report = member.run.context.halt_report().unwrap();
    let Some(Payload::Fault(payload)) = &report.event().unwrap().payload else {
        panic!("retained fault");
    };
    let fault: weaver_types::FaultReport = serde_json::from_str(payload.get()).unwrap();
    assert_eq!(fault.case, weaver_types::FaultCase::StreamWriteFailed);
    assert!(report.fault_sequence().is_none());
    assert!(report.trace_unavailable());
    let pending = member.run.context.pending_author().unwrap().ticket;
    let secondary = member.run.context.halt().unwrap().secondary().len();
    for _ in 0..4 {
        assert_eq!(advance(&mut member), HaltTraceProgress::Unavailable);
    }
    assert_eq!(
        member.run.context.halt().unwrap().secondary().len(),
        secondary
    );
    assert_eq!(member.run.context.pending_author().unwrap().ticket, pending);
    assert_eq!(member.run.context.failure(), Some(&first));
}

#[test]
fn halt_report_failed_sink_without_pending_work_stays_unavailable() {
    let mut member = Member::new();
    member.fill_trace();
    assert!(member.run.context.pending_author().is_none());
    member.run._reader.shutdown(Shutdown::Both).unwrap();
    let watch = Instant::now() + Duration::from_secs(5);
    loop {
        let context = &mut member.run.context;
        if context.barrier.drain_status(&mut context.budget).is_err() {
            break;
        }
        assert!(
            Instant::now() < watch,
            "writer did not detect the closed sink"
        );
        std::thread::yield_now();
    }
    let first = member
        .run
        .context
        .reduce_flow(&mut member.run.clock)
        .unwrap_err();
    begin_report(&mut member);
    assert_eq!(advance(&mut member), HaltTraceProgress::Unavailable);
    assert!(
        member
            .run
            .context
            .halt_report()
            .unwrap()
            .trace_unavailable()
    );
    let secondary = member.run.context.halt().unwrap().secondary().len();
    for _ in 0..4 {
        assert_eq!(advance(&mut member), HaltTraceProgress::Unavailable);
    }
    assert_eq!(
        member.run.context.halt().unwrap().secondary().len(),
        secondary
    );
    assert!(member.run.context.pending_author().is_none());
    assert_eq!(member.run.context.failure(), Some(&first));
}

#[test]
fn halt_report_secondary_evidence_cannot_rewrite_the_first_fault_or_deadline() {
    let mut member = Member::new();
    bound_failure(&mut member);
    begin_report(&mut member);
    let first = member.run.context.halt().unwrap().first().clone();
    let deadline = member.run.context.halt().unwrap().deadline();
    let account = member
        .run
        .context
        .halt()
        .unwrap()
        .fault_account()
        .unwrap()
        .get()
        .to_owned();
    assert!(matches!(
        advance(&mut member),
        HaltTraceProgress::FaultRecorded(_)
    ));
    member.run.clock.advance_to(deadline.unwrap()).unwrap();
    assert_eq!(advance(&mut member), HaltTraceProgress::AbortRequired);
    let halt = member.run.context.halt().unwrap();
    assert_eq!(halt.first(), &first);
    assert_eq!(halt.deadline(), deadline);
    assert_eq!(halt.fault_account().unwrap().get(), account);
    let unload: serde_json::Value =
        serde_json::from_str(halt.unload_account().unwrap().get()).unwrap();
    assert_eq!(unload["secondary"].as_array().unwrap().len(), 1);
    assert!(member.records(Kind::Unload).is_empty());
}

#[test]
fn halt_report_deadline_overflow_never_invents_a_new_anchor() {
    let mut member = Member::new();
    member.run.clock.advance_to(u64::MAX - 2).unwrap();
    bound_failure(&mut member);
    assert!(member.run.context.halt().unwrap().deadline().is_err());
    begin_report(&mut member);
    assert_eq!(advance(&mut member), HaltTraceProgress::AbortRequired);
    assert!(member.run.context.halt_report().unwrap().event().is_none());
    assert!(member.records(Kind::Fault).is_empty());
    assert!(member.run.context.halt().unwrap().deadline().is_err());
}

#[test]
fn halt_report_initial_clock_failure_never_spends_the_fault_permit() {
    struct Broken;
    impl PumpIo for Broken {
        fn now(&mut self) -> std::io::Result<u64> {
            Err(std::io::Error::other("clock lost"))
        }
        fn wait(&mut self, _: &[WaitInterest<'_>], _: Option<u64>) -> std::io::Result<WaitOutcome> {
            panic!("report may not wait");
        }
    }
    let mut member = Member::new();
    let first = bound_failure(&mut member);
    begin_report(&mut member);
    assert_eq!(
        member.run.context.advance_halt_trace(&mut Broken).unwrap(),
        HaltTraceProgress::AbortRequired
    );
    assert!(member.run.context.halt_report().unwrap().event().is_none());
    assert!(
        member
            .run
            .context
            .halt_report()
            .unwrap()
            .fault_sequence()
            .is_none()
    );
    assert!(member.records(Kind::Fault).is_empty());
    assert_eq!(member.run.context.failure(), Some(&first));
    assert_eq!(
        member.run.context.halt().unwrap().secondary()[0].observed_ns,
        None
    );
    assert_eq!(advance(&mut member), HaltTraceProgress::AbortRequired);
}

const _: fn() = || {
    trait NotClone<M> {
        fn pin() {}
    }
    impl<T: ?Sized> NotClone<()> for T {}
    impl<T: Clone> NotClone<u8> for T {}
    let _ = <HaltReport as NotClone<_>>::pin;
    trait NotDefault<M> {
        fn pin() {}
    }
    impl<T: ?Sized> NotDefault<()> for T {}
    impl<T: Default> NotDefault<u8> for T {}
    let _ = <HaltReport as NotDefault<_>>::pin;
    trait NotFromSequence<M> {
        fn pin() {}
    }
    impl<T: ?Sized> NotFromSequence<()> for T {}
    impl<T: From<Sequence>> NotFromSequence<u8> for T {}
    let _ = <HaltReport as NotFromSequence<_>>::pin;
};
