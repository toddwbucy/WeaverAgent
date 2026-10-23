use super::*;
use crate::flow::author::AuthoredSource;
use crate::flow::clock::{WaitInterest, WaitOutcome};
use crate::flow::context::tests::{Fixture, config, event};
use crate::flow::halt_snapshot::ExternalFacts;
use crate::flow::pump::LifecyclePorts;
use std::io::{self, Read};
use std::time::{Duration, Instant};
use weaver_trace::{Subsystem, TurnRef};

const _: fn() = || {
    trait Private<M> {
        fn pin() {}
    }
    impl<T: ?Sized> Private<()> for T {}
    impl<T: Clone> Private<u8> for T {}
    impl<T: Default> Private<u16> for T {}
    impl<T: From<u64>> Private<u32> for T {}
    impl<T: From<AuthoredEvent>> Private<u64> for T {}
    let _ = <ReleaseAttempt as Private<_>>::pin;
    let _ = <ReleaseAuthor<'static> as Private<_>>::pin;
    let _ = <ReleaseScope<'static> as Private<_>>::pin;
    let _ = <ReleaseTicket as Private<_>>::pin;
};

fn shutdown(kind: Kind) -> AuthoredEvent {
    let payload = match kind {
        Kind::SessionClosed => None,
        Kind::Fault => Some(Payload::Fault(
            serde_json::value::to_raw_value(&weaver_types::FaultReport {
                case: weaver_types::FaultCase::OrganDeathObserved,
                account: serde_json::value::to_raw_value(
                    &serde_json::json!({"fixture":"release organ death"}),
                )
                .unwrap(),
            })
            .unwrap(),
        )),
        Kind::Refusal => Some(Payload::Refusal(
            serde_json::value::to_raw_value(&weaver_types::LifecycleRefusal::OutOfOrder).unwrap(),
        )),
        Kind::Unload => Some(Payload::Unload(Box::new(serving::payloads::UnloadClose {
            grant_surface: None,
            cause: None,
            forced: false,
            forced_by: None,
            release: None,
            end_reason: None,
            state_flow_failure: None,
        }))),
        _ => panic!("not a shutdown fixture"),
    };
    event(kind, payload)
}
fn ports<I: PumpIo>(io: &mut I) -> PumpPorts<'_, I> {
    PumpPorts {
        lifecycle: LifecyclePorts::default(),
        gate: None,
        decode: None,
        label: None,
        io,
    }
}
fn fill(fx: &mut Fixture) {
    let payload =
        serde_json::value::to_raw_value(&serde_json::json!({"fixture":"x".repeat(32768)})).unwrap();
    for _ in 0..serving::ORDINARY_CAPACITY + 64 {
        if fx.context.barrier.recorder().boundary().queued == serving::ORDINARY_CAPACITY {
            return;
        }
        let event = AuthoredEvent {
            kind: Kind::ModelMeasurement,
            subsystem: Subsystem::Harness,
            turn: Some(TurnRef("pressure".into())),
            source: AuthoredSource::Runtime,
            causal_parent: None,
            payload: Some(Payload::ModelMeasurement(payload.clone())),
        };
        assert!(matches!(
            fx.context.barrier.submit_authored_with_clock(
                event,
                RecordPurpose::Observation,
                &fx.context.author,
                1,
                &mut fx.context.inbox,
                &mut fx.context.budget,
                &mut || fx.clock.now()
            ),
            RecordProgress::Recorded { .. }
        ));
    }
    panic!("fixture did not fill actual ordinary trace capacity");
}
fn releasing(stopped: bool, pressure: bool) -> Fixture {
    let mut fx = Fixture::new(config());
    fx.activate();
    fx.context.complete_wait(PumpDeadline::Enter).unwrap();
    if pressure {
        fill(&mut fx);
    }
    let leave = AbsoluteBound::after(fx.clock.now().unwrap(), 1_000_000).unwrap();
    fx.context.begin_releasing(leave, &mut fx.clock).unwrap();
    assert_eq!(
        fx.context.retire_state(&mut fx.clock).unwrap(),
        Advance::Progress
    );
    assert_eq!(
        fx.context.retire_classifier(&mut fx.clock).unwrap(),
        Advance::Progress
    );
    if stopped {
        fx.context.barrier.stop_for_flow_failure();
    }
    fx
}
fn drain(fx: &mut Fixture) {
    fx._reader.set_nonblocking(true).unwrap();
    let end = Instant::now() + Duration::from_secs(5);
    let mut bytes = [0; 65536];
    while fx.context.barrier.recorder().boundary().queued != 0 {
        assert!(Instant::now() < end, "real writer did not drain");
        match fx._reader.read(&mut bytes) {
            Ok(0) => panic!("trace closed before drain"),
            Ok(_) => {}
            Err(error) if error.kind() == io::ErrorKind::WouldBlock => std::thread::yield_now(),
            Err(error) => panic!("trace drain: {error}"),
        }
    }
}
fn original_pointer(event: &AuthoredEvent) -> usize {
    match event.payload.as_ref().unwrap() {
        Payload::Fault(raw) | Payload::Refusal(raw) => raw.get().as_ptr() as usize,
        Payload::Unload(raw) => (&**raw as *const serving::payloads::UnloadClose) as usize,
        _ => panic!("unexpected original"),
    }
}
fn submit(fx: &mut Fixture, event: AuthoredEvent) -> RecordProgress {
    fx.context
        .submit_context_author(event, RecordPurpose::Observation, 1, &mut fx.clock)
}

#[test]
fn release_scope_records_the_closed_shutdown_set_with_live_or_stopped_tap() {
    for stopped in [false, true] {
        let mut fx = releasing(stopped, false);
        let before = fx.context.barrier.recorder().structure().len();
        let mut sequences = Vec::new();
        for kind in [
            Kind::Fault,
            Kind::Refusal,
            Kind::SessionClosed,
            Kind::Unload,
        ] {
            sequences.push(fx.author(shutdown(kind)).unwrap());
            assert!(fx.context.failure().is_none());
            assert_eq!(fx.context.budget.gauges(), (0, 0));
            assert_eq!(fx.context.inbox.queued().count(), 0);
            assert_eq!(fx.context.flow.ledger().facts().count(), 0);
            assert!(fx.context.release_author.is_none());
        }
        assert!(sequences.windows(2).all(|pair| pair[0].0 + 1 == pair[1].0));
        assert_eq!(fx.context.barrier.recorder().structure().len(), before + 4);
        assert_eq!(fx.context.barrier.recorder().tap_stopped(), stopped);
        drain(&mut fx);
        fx.context
            .close_after_drain(&mut ports(&mut fx.clock))
            .unwrap();
        let count = fx.context.barrier.recorder().structure().len();
        assert_eq!(fx.author(shutdown(Kind::Fault)), Err(RunFailure::Closed));
        assert!(fx.context.reduce_flow(&mut fx.clock).is_err());
        assert_eq!(fx.context.barrier.recorder().structure().len(), count);
    }
}

#[test]
fn release_shutdown_uses_ordinary_capacity_and_first_failed_fault_keeps_its_reserve() {
    for kind in [Kind::Fault, Kind::SessionClosed, Kind::Unload] {
        let mut fx = releasing(true, true);
        let before = fx.context.barrier.recorder().structure().len();
        assert!(matches!(
            submit(&mut fx, shutdown(kind)),
            RecordProgress::Pending
        ));
        let ticket = fx.context.pending_author().unwrap().ticket;
        assert_eq!(fx.context.barrier.recorder().structure().len(), before);
        let first = fx.context.fail(
            &mut fx.clock,
            FlowHaltReason::ProtocolFailed,
            HaltPoint::Author,
            "actual failure after release pressure",
        );
        fx.context
            .freeze_halt_account(ExternalFacts {
                outstanding_pairs: vec![],
                capture: None,
                wait: None,
                input_id: None,
            })
            .unwrap();
        fx.context.begin_halt_report().unwrap();
        assert!(matches!(
            fx.context.advance_halt_trace(&mut fx.clock).unwrap(),
            HaltTraceProgress::FaultRecorded(_)
        ));
        assert_eq!(fx.context.pending_author().unwrap().ticket, ticket);
        assert_eq!(fx.context.barrier.recorder().structure().len(), before + 1);
        assert!(matches!(
            fx.context.resume_context_author(&mut fx.clock),
            RecordProgress::Failed
        ));
        assert_eq!(fx.context.failure(), Some(&first));
        assert_eq!(fx.context.barrier.recorder().structure().len(), before + 1);
    }
}

#[test]
fn release_pressure_and_recoverable_wait_keep_exact_original_ticket_until_one_record() {
    let mut fx = releasing(true, true);
    let before = fx.context.barrier.recorder().structure().len();
    let original = shutdown(Kind::Refusal);
    let pointer = original_pointer(&original);
    let bound = AbsoluteBound::after(fx.clock.now().unwrap(), 10).unwrap();
    fx.context
        .arm(PumpDeadline::Label, bound, &mut fx.clock)
        .unwrap();
    assert_eq!(
        fx.author(original),
        Err(RunFailure::Wait {
            key: PumpDeadline::Label,
            bound
        })
    );
    let ticket = fx.context.pending_author().unwrap().ticket;
    assert_eq!(
        original_pointer(&fx.context.pending_author().unwrap().event),
        pointer
    );
    assert!(fx.context.failure().is_none());
    assert_eq!(fx.context.barrier.recorder().structure().len(), before);
    fx.context.complete_wait(PumpDeadline::Label).unwrap();
    drain(&mut fx);
    let sequence = fx
        .context
        .resume_author_and_wait(&mut ports(&mut fx.clock))
        .unwrap();
    assert!(sequence.0 > 0);
    assert!(fx.context.pending_author().is_none());
    assert!(fx.context.release_author.is_none());
    assert_eq!(fx.context.barrier.recorder().structure().len(), before + 1);
    let recorded = fx
        .context
        .barrier
        .recorder()
        .structure()
        .by_kind(Kind::Refusal)
        .last()
        .unwrap();
    assert!(recorded.line.contains("out_of_order"));
    assert!(ticket > 0);
}

struct BrokenClock;
impl PumpIo for BrokenClock {
    fn now(&mut self) -> io::Result<u64> {
        Err(io::Error::other("release pre-prepare clock"))
    }
    fn wait(&mut self, _: &[WaitInterest<'_>], _: Option<u64>) -> io::Result<WaitOutcome> {
        unreachable!()
    }
}
#[test]
fn release_preparation_clock_and_attribution_failure_retain_the_original_before_id() {
    for clock_failure in [false, true] {
        let mut fx = releasing(true, false);
        let before = fx.context.barrier.recorder().structure().len();
        let mut original = shutdown(Kind::Fault);
        if !clock_failure {
            original.source = AuthoredSource::Loop;
        }
        let pointer = original_pointer(&original);
        let progress = if clock_failure {
            fx.context.submit_context_author(
                original,
                RecordPurpose::Observation,
                1,
                &mut BrokenClock,
            )
        } else {
            submit(&mut fx, original)
        };
        assert!(matches!(progress, RecordProgress::Failed));
        assert!(fx.context.barrier.pending().is_none());
        let ticket = fx.context.pending_author().unwrap().ticket;
        assert_eq!(
            original_pointer(&fx.context.pending_author().unwrap().event),
            pointer
        );
        assert!(fx.context.release_author.is_some());
        assert!(fx.context.barrier.admitted_before_failure().is_none());
        assert_eq!(fx.context.barrier.recorder().structure().len(), before);
        let first = fx.context.record_failed(&mut fx.clock);
        assert_eq!(
            fx.context.resume_author_and_wait(&mut ports(&mut fx.clock)),
            Err(first.clone())
        );
        assert_eq!(fx.context.pending_author().unwrap().ticket, ticket);
        assert_eq!(fx.context.failure(), Some(&first));
    }
}

#[test]
fn release_scope_cannot_be_borrowed_by_another_context_with_identical_trace_ids() {
    let mut first = releasing(true, false);
    let mut second = releasing(true, false);
    first.context.release_author = Some(ReleaseAttempt {
        identity: Arc::new(()),
        recorded: None,
    });
    let before = second.context.barrier.recorder().structure().len();
    let original = shutdown(Kind::Fault);
    let pointer = original_pointer(&original);
    let scope = ReleaseScope {
        barrier: first.context.barrier.author_identity(),
        phase: &first.context.phase,
        failure: &first.context.failure,
        attempt: &first.context.release_author,
    };
    let progress = second.context.barrier.submit_release_authored_with_clock(
        ReleaseAuthor {
            scope,
            event: original,
        },
        RecordPurpose::Observation,
        &second.context.author,
        1,
        &mut second.context.inbox,
        &mut second.context.budget,
        &mut || second.clock.now(),
    );
    let RecordProgress::BusyAuthored { event, .. } = progress else {
        panic!("foreign seal admitted")
    };
    assert_eq!(original_pointer(&event), pointer);
    assert_eq!(second.context.barrier.recorder().structure().len(), before);
    assert!(second.context.pending_author().is_none());
    assert!(first.context.failure().is_none());
}

#[test]
fn release_scope_requires_the_actual_releasing_phase_and_no_latched_failure() {
    for phase in [
        RecordingPhase::Preflight,
        RecordingPhase::Entered,
        RecordingPhase::Failed,
        RecordingPhase::Closed,
    ] {
        let mut fx = if phase == RecordingPhase::Preflight {
            Fixture::new(config())
        } else {
            releasing(true, false)
        };
        if phase == RecordingPhase::Entered {
            fx = Fixture::new(config());
            fx.activate();
        } else if phase == RecordingPhase::Failed {
            fx.context.fail(
                &mut fx.clock,
                FlowHaltReason::ProtocolFailed,
                HaltPoint::Author,
                "first",
            );
        } else if phase == RecordingPhase::Closed {
            fx.author(shutdown(Kind::Unload)).unwrap();
            drain(&mut fx);
            fx.context
                .close_after_drain(&mut ports(&mut fx.clock))
                .unwrap();
        }
        assert_eq!(fx.context.phase(), phase);
        let first = fx.context.failure().cloned();
        fx.context.release_author = Some(ReleaseAttempt {
            identity: Arc::new(()),
            recorded: None,
        });
        let scope = ReleaseScope {
            barrier: fx.context.barrier.author_identity(),
            phase: &fx.context.phase,
            failure: &fx.context.failure,
            attempt: &fx.context.release_author,
        };
        assert!(matches!(
            fx.context.barrier.submit_release_authored_with_clock(
                ReleaseAuthor {
                    scope,
                    event: shutdown(Kind::SessionClosed)
                },
                RecordPurpose::Observation,
                &fx.context.author,
                1,
                &mut fx.context.inbox,
                &mut fx.context.budget,
                &mut || fx.clock.now()
            ),
            RecordProgress::BusyAuthored { .. }
        ));
        assert!(
            matches!(fx.context.barrier.failure(), Some(RecordFailure::Protocol(detail)) if detail == "invalid release author scope")
        );
        assert!(fx.context.pending_author().is_none());
        assert_eq!(fx.context.failure(), first.as_ref());
    }
}

#[test]
fn release_scope_refuses_nonshutdown_kinds_selected_purpose_and_malformed_shutdown() {
    for kind in [
        Kind::TurnStarted,
        Kind::Recall,
        Kind::StatePlacement,
        Kind::StateMark,
        Kind::ModelMeasurement,
        Kind::MessageUser,
    ] {
        let mut fx = releasing(true, false);
        let before = fx.context.barrier.recorder().structure().len();
        let mut original = event(kind, None);
        original.turn = Some(TurnRef("would-be-new-turn".into()));
        assert!(matches!(
            submit(&mut fx, original),
            RecordProgress::BusyAuthored { .. }
        ));
        assert_eq!(fx.context.barrier.recorder().structure().len(), before);
        assert_eq!(fx.context.budget.gauges(), (0, 0));
        assert_eq!(fx.context.inbox.queued().count(), 0);
        assert_eq!(fx.context.flow.ledger().facts().count(), 0);
    }
    let mut fx = releasing(true, false);
    let before = fx.context.barrier.recorder().structure().len();
    assert!(matches!(
        fx.context.submit_context_author(
            shutdown(Kind::SessionClosed),
            RecordPurpose::Content,
            1,
            &mut fx.clock
        ),
        RecordProgress::BusyAuthored { .. }
    ));
    assert_eq!(fx.context.barrier.recorder().structure().len(), before);
    let mut fx = releasing(true, false);
    let before = fx.context.barrier.recorder().structure().len();
    let malformed = event(Kind::Fault, shutdown(Kind::Refusal).payload);
    assert!(matches!(submit(&mut fx, malformed), RecordProgress::Failed));
    assert!(fx.context.pending_author().is_some());
    assert_eq!(fx.context.barrier.recorder().structure().len(), before);
}

#[test]
fn release_scope_cannot_resume_another_attempt_or_a_replacement_ticket() {
    for replaced_ticket in [false, true] {
        let mut fx = releasing(true, true);
        assert!(matches!(
            submit(&mut fx, shutdown(Kind::Fault)),
            RecordProgress::Pending
        ));
        let old = fx.context.pending_author().unwrap().ticket;
        if replaced_ticket {
            let new = fx.context.barrier.replace_pending_author_ticket_for_test();
            assert_ne!(old, new);
        } else {
            fx.context.release_author = Some(ReleaseAttempt {
                identity: Arc::new(()),
                recorded: None,
            });
        }
        let before = fx.context.barrier.recorder().structure().len();
        assert!(matches!(
            fx.context.resume_context_author(&mut fx.clock),
            RecordProgress::Failed
        ));
        assert_eq!(fx.context.barrier.recorder().structure().len(), before);
        assert!(fx.context.pending_author().is_some());
    }
}

#[test]
fn generic_and_raw_barrier_roads_cannot_borrow_stopped_release_permission() {
    for raw in [false, true] {
        let mut fx = releasing(true, false);
        let before = fx.context.barrier.recorder().structure().len();
        let original = shutdown(Kind::SessionClosed);
        let progress = if raw {
            let prepared = fx
                .context
                .author
                .prepare(&original, fx.clock.now().unwrap(), 1)
                .unwrap();
            fx.context.barrier.submit_with_clock(
                prepared,
                RecordPurpose::Observation,
                &mut fx.context.inbox,
                &mut fx.context.budget,
                &mut || fx.clock.now(),
            )
        } else {
            fx.context.barrier.submit_authored_with_clock(
                original,
                RecordPurpose::Observation,
                &fx.context.author,
                1,
                &mut fx.context.inbox,
                &mut fx.context.budget,
                &mut || fx.clock.now(),
            )
        };
        assert!(matches!(progress, RecordProgress::Failed));
        assert_eq!(fx.context.barrier.recorder().structure().len(), before);
    }
    let mut fx = releasing(false, true);
    assert!(matches!(
        submit(&mut fx, shutdown(Kind::SessionClosed)),
        RecordProgress::Pending
    ));
    assert!(matches!(
        fx.context.barrier.resume_with_clock(
            &mut fx.context.inbox,
            &mut fx.context.budget,
            &mut || fx.clock.now()
        ),
        RecordProgress::Failed
    ));
    assert!(fx.context.pending_author().is_some());
}

#[test]
fn release_scope_binds_run_identity_and_completed_attempt_cannot_replay() {
    let mut fx = releasing(true, false);
    let before = fx.context.barrier.recorder().structure().len();
    let other = ServingAuthor::new(
        "session".into(),
        "different-run".into(),
        42,
        fx.context.origin_ns(),
    )
    .unwrap();
    fx.context.release_author = Some(ReleaseAttempt {
        identity: Arc::new(()),
        recorded: None,
    });
    let scope = ReleaseScope {
        barrier: fx.context.barrier.author_identity(),
        phase: &fx.context.phase,
        failure: &fx.context.failure,
        attempt: &fx.context.release_author,
    };
    assert!(matches!(
        fx.context.barrier.submit_release_authored_with_clock(
            ReleaseAuthor {
                scope,
                event: shutdown(Kind::Fault)
            },
            RecordPurpose::Observation,
            &other,
            1,
            &mut fx.context.inbox,
            &mut fx.context.budget,
            &mut || fx.clock.now()
        ),
        RecordProgress::Failed
    ));
    assert_eq!(fx.context.barrier.recorder().structure().len(), before);
    let mut fx = releasing(true, false);
    let RecordProgress::Recorded {
        sequence,
        proof: None,
    } = submit(&mut fx, shutdown(Kind::Fault))
    else {
        panic!("release record expected")
    };
    fx.context.retain_recorded_author(sequence);
    let before = fx.context.barrier.recorder().structure().len();
    assert!(matches!(
        fx.context.resume_context_author(&mut fx.clock),
        RecordProgress::Failed
    ));
    assert_eq!(fx.context.completed_author, Some(sequence));
    assert_eq!(fx.context.barrier.recorder().structure().len(), before);
}

struct ReleaseBoundaryClock {
    before: u64,
    after: u64,
    boundary: usize,
    samples: usize,
    broken: bool,
}
impl PumpIo for ReleaseBoundaryClock {
    fn now(&mut self) -> io::Result<u64> {
        self.samples += 1;
        if self.samples >= self.boundary {
            if self.broken {
                Err(io::Error::other("release post-Recorded clock"))
            } else {
                Ok(self.after)
            }
        } else {
            Ok(self.before)
        }
    }
    fn wait(&mut self, _: &[WaitInterest<'_>], _: Option<u64>) -> io::Result<WaitOutcome> {
        panic!("unpressured shutdown author must not wait for transport")
    }
}
#[test]
fn release_recorded_sequence_survives_later_clock_failure_or_recoverable_wait() {
    for broken in [false, true] {
        let mut witnessed = false;
        for boundary in 1..20 {
            let mut fx = releasing(true, false);
            let before = fx.context.barrier.recorder().structure().len();
            let now = fx.clock.now().unwrap();
            let wait = AbsoluteBound::after(now, 10).unwrap();
            fx.context
                .arm(PumpDeadline::Label, wait, &mut fx.clock)
                .unwrap();
            let mut io = ReleaseBoundaryClock {
                before: now,
                after: wait.deadline_ns(),
                boundary,
                samples: 0,
                broken,
            };
            let outcome = fx
                .context
                .author_and_wait(&mut ports(&mut io), shutdown(Kind::SessionClosed));
            if fx.context.barrier.recorder().structure().len() != before + 1 || outcome.is_ok() {
                continue;
            }
            witnessed = true;
            let sequence = fx
                .context
                .barrier
                .recorder()
                .structure()
                .by_kind(Kind::SessionClosed)
                .last()
                .unwrap()
                .sequence;
            assert_eq!(fx.context.last_author_sequence, Some(sequence));
            assert_eq!(fx.context.completed_author, Some(sequence));
            assert_eq!(
                fx.context.release_author.as_ref().unwrap().recorded,
                Some(sequence)
            );
            assert!(fx.context.pending_author().is_none());
            if broken {
                let first = outcome.unwrap_err();
                assert_eq!(fx.context.failure(), Some(&first));
                assert_eq!(
                    fx.context.resume_author_and_wait(&mut ports(&mut fx.clock)),
                    Err(first.clone())
                );
                assert_eq!(fx.context.failure(), Some(&first));
                assert_eq!(fx.context.completed_author, Some(sequence));
            } else {
                assert_eq!(
                    outcome,
                    Err(RunFailure::Wait {
                        key: PumpDeadline::Label,
                        bound: wait
                    })
                );
                assert!(fx.context.failure().is_none());
                fx.clock.advance_to(wait.deadline_ns()).unwrap();
                fx.context.complete_wait(PumpDeadline::Label).unwrap();
                assert_eq!(
                    fx.context.resume_author_and_wait(&mut ports(&mut fx.clock)),
                    Ok(sequence)
                );
                assert!(fx.context.completed_author.is_none());
                assert!(fx.context.release_author.is_none());
            }
            assert_eq!(fx.context.barrier.recorder().structure().len(), before + 1);
            break;
        }
        assert!(
            witnessed,
            "must actually cross Recorded before the later failure/wait"
        );
    }
}

#[test]
fn release_cannot_reopen_an_already_unloaded_recorder() {
    let mut fx = releasing(true, false);
    fx.author(shutdown(Kind::Unload)).unwrap();
    assert_eq!(fx.context.phase(), RecordingPhase::Releasing);
    assert!(fx.context.barrier.is_closed());
    let before = fx.context.barrier.recorder().structure().len();
    assert!(fx.author(shutdown(Kind::Fault)).is_err());
    assert_eq!(fx.context.phase(), RecordingPhase::Failed);
    assert!(fx.context.barrier.recorder().tap_stopped());
    assert_eq!(fx.context.barrier.recorder().structure().len(), before);
}

#[test]
fn release_second_caller_preserves_the_first_attempt_and_both_originals() {
    let mut fx = releasing(true, true);
    assert!(matches!(
        submit(&mut fx, shutdown(Kind::Fault)),
        RecordProgress::Pending
    ));
    let ticket = fx.context.pending_author().unwrap().ticket;
    let attempt = Arc::clone(&fx.context.release_author.as_ref().unwrap().identity);
    let original = shutdown(Kind::Refusal);
    let pointer = original_pointer(&original);
    let before = fx.context.barrier.recorder().structure().len();
    let failure = fx.author(original).unwrap_err();
    assert_eq!(fx.context.failure(), Some(&failure));
    assert_eq!(fx.context.pending_author().unwrap().ticket, ticket);
    assert!(Arc::ptr_eq(
        &attempt,
        &fx.context.release_author.as_ref().unwrap().identity
    ));
    assert_eq!(
        original_pointer(fx.context.refused_author().unwrap()),
        pointer
    );
    assert_eq!(fx.context.barrier.recorder().structure().len(), before);
}

#[test]
fn release_resume_routes_an_ordinary_pending_ticket_without_a_release_attempt() {
    // Defensive owner test: public begin_releasing refuses a pending author.
    // A context already in Releasing must still resume by the retained ticket.
    let mut fx = releasing(false, true);
    let original = shutdown(Kind::Fault);
    let pointer = original_pointer(&original);
    let before = fx.context.barrier.recorder().structure().len();
    assert!(matches!(
        fx.context.barrier.submit_authored_with_clock(
            original,
            RecordPurpose::Observation,
            &fx.context.author,
            1,
            &mut fx.context.inbox,
            &mut fx.context.budget,
            &mut || fx.clock.now(),
        ),
        RecordProgress::Pending
    ));
    assert_eq!(fx.context.phase(), RecordingPhase::Releasing);
    assert!(fx.context.release_author.is_none());
    assert_eq!(
        original_pointer(&fx.context.pending_author().unwrap().event),
        pointer
    );
    drain(&mut fx);
    assert!(matches!(
        fx.context.resume_context_author(&mut fx.clock),
        RecordProgress::Recorded { proof: None, .. }
    ));
    assert!(fx.context.barrier.failure().is_none());
    assert!(fx.context.pending_author().is_none());
    assert!(fx.context.release_author.is_none());
    assert_eq!(fx.context.barrier.recorder().structure().len(), before + 1);
}

#[test]
fn release_owed_boot_defense_returns_typed_refusal_without_admission_or_halt() {
    for kind in [
        Kind::Fault,
        Kind::Refusal,
        Kind::SessionClosed,
        Kind::Unload,
    ] {
        let mut fx = releasing(true, false);
        // Public admission refuses BootOpenOwned before reaching this defense.
        fx.context.barrier.set_boot_open_owed(true);
        let original = shutdown(kind);
        let expected = format!("{original:?}");
        let before = fx.context.barrier.recorder().structure().len();
        let progress = fx.context.submit_context_author(
            original,
            RecordPurpose::Observation,
            1,
            &mut BrokenClock,
        );
        assert!(matches!(
            &progress,
            RecordProgress::BusyAuthored {
                owed: Some(OwedAuthor::BootOpen),
                purpose: RecordPurpose::Observation,
                ..
            }
        ));
        assert!(fx.context.pending_author().is_none());
        assert!(fx.context.barrier.pending().is_none());
        assert_eq!(
            fx.context
                .wait_recorded(&mut ports(&mut fx.clock), progress),
            Err(RunFailure::BootOpenOwned)
        );
        assert_eq!(
            format!("{:?}", fx.context.refused_author().unwrap()),
            expected
        );
        assert!(fx.context.failure().is_none());
        assert!(fx.context.barrier.failure().is_none());
        assert!(!fx.context.budget.is_frozen());
        assert!(fx.context.barrier.boot_open_owed());
        assert!(fx.context.barrier.admitted_before_failure().is_none());
        assert!(fx.context.release_author.is_none());
        assert_eq!(fx.context.barrier.recorder().structure().len(), before);
        fx.context.barrier.set_boot_open_owed(false);
        fx.author(shutdown(kind)).unwrap();
        assert_eq!(fx.context.barrier.recorder().structure().len(), before + 1);
    }
}

#[test]
fn release_pending_mark_requires_its_scope_even_when_the_tap_is_live() {
    let mut fx = releasing(false, true);
    assert!(matches!(
        submit(&mut fx, shutdown(Kind::Fault)),
        RecordProgress::Pending
    ));
    let ticket = fx.context.pending_author().unwrap().ticket;
    let before = fx.context.barrier.recorder().structure().len();
    drain(&mut fx);
    // Isolate scope validation from the stopped-tap check.
    fx.context.phase = RecordingPhase::Entered;
    assert!(matches!(
        fx.context.resume_context_author(&mut fx.clock),
        RecordProgress::Failed
    ));
    assert_eq!(fx.context.pending_author().unwrap().ticket, ticket);
    assert_eq!(fx.context.barrier.recorder().structure().len(), before);
    assert!(
        matches!(fx.context.barrier.failure(), Some(RecordFailure::Protocol(detail))
        if detail == "release author lost its exact scope or ticket")
    );
}

#[test]
fn release_returned_original_survives_a_failing_intake_before_the_hold_result() {
    for owed in [false, true] {
        let mut fx = releasing(true, false);
        fx.context.barrier.set_boot_open_owed(owed);
        let original = shutdown(Kind::Fault);
        let pointer = original_pointer(&original);
        let expected = format!("{original:?}");
        let before = fx.context.barrier.recorder().structure().len();
        let progress = fx.context.submit_context_author(
            original,
            if owed {
                RecordPurpose::Observation
            } else {
                RecordPurpose::Content
            },
            1,
            &mut fx.clock,
        );
        assert!(matches!(&progress, RecordProgress::BusyAuthored { .. }));
        if owed {
            // A real separate author fails its clock after the account hold
            // returned the first original. The hold must not mask that failure.
            assert!(matches!(
                fx.context.barrier.submit_authored_with_clock(
                    shutdown(Kind::Refusal),
                    RecordPurpose::Observation,
                    &fx.context.author,
                    1,
                    &mut fx.context.inbox,
                    &mut fx.context.budget,
                    &mut || Err(io::Error::other("later author clock")),
                ),
                RecordProgress::Failed
            ));
            assert!(matches!(
                fx.context.barrier.failure(),
                Some(RecordFailure::Clock(_))
            ));
        } else {
            // The invalid release purpose really returns the sole original
            // and latches the barrier failure before wait_recorded observes it.
            assert!(matches!(
                fx.context.barrier.failure(),
                Some(RecordFailure::Protocol(_))
            ));
        }
        let failure = fx
            .context
            .wait_recorded(&mut ports(&mut fx.clock), progress)
            .unwrap_err();
        assert!(matches!(&failure, RunFailure::Flow { reason, .. }
            if *reason == if owed { FlowHaltReason::TraceFailed } else { FlowHaltReason::ProtocolFailed }));
        assert_eq!(fx.context.failure(), Some(&failure));
        assert_eq!(
            original_pointer(fx.context.refused_author().unwrap()),
            pointer
        );
        assert_eq!(
            format!("{:?}", fx.context.refused_author().unwrap()),
            expected
        );
        assert_eq!(fx.context.barrier.recorder().structure().len(), before);
        assert!(fx.context.barrier.admitted_before_failure().is_none());
        assert_eq!(fx.context.phase(), RecordingPhase::Failed);
    }
}
