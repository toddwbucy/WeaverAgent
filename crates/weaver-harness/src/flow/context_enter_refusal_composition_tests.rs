use super::*;
use crate::flow::clock::testing::FakePumpIo;
use std::io::Read;
use std::os::unix::net::UnixStream;
use std::time::{Duration, Instant};
use weaver_types::state_flow as s;

fn ports(io: &mut FakePumpIo) -> PumpPorts<'_, FakePumpIo> {
    PumpPorts {
        lifecycle: LifecyclePorts::default(),
        gate: None,
        decode: None,
        label: None,
        io,
    }
}

pub(in crate::flow::context::startup_composition) fn fill(
    context: &mut RecordingContext,
    io: &mut FakePumpIo,
) {
    let payload =
        serde_json::value::to_raw_value(&serde_json::json!({"pressure": "x".repeat(32_768)}))
            .unwrap();
    for _ in 0..weaver_trace::serving::ORDINARY_CAPACITY + 64 {
        if context.barrier.recorder().boundary().queued == weaver_trace::serving::ORDINARY_CAPACITY
        {
            return;
        }
        let now = io.now().unwrap();
        assert!(matches!(
            context.barrier.submit_authored_with_clock(
                AuthoredEvent {
                    kind: Kind::ModelMeasurement,
                    subsystem: Subsystem::Harness,
                    turn: Some(weaver_trace::TurnRef("pressure".into())),
                    source: AuthoredSource::Runtime,
                    causal_parent: None,
                    payload: Some(Payload::ModelMeasurement(payload.clone()))
                },
                RecordPurpose::Observation,
                &context.author,
                1,
                &mut context.inbox,
                &mut context.budget,
                &mut || Ok(now),
            ),
            RecordProgress::Recorded { .. }
        ));
    }
    panic!("actual ordinary queue did not fill")
}

pub(in crate::flow::context::startup_composition) fn drain(
    context: &RecordingContext,
    reader: &mut UnixStream,
) {
    reader.set_nonblocking(true).unwrap();
    let watchdog = Instant::now() + Duration::from_secs(3);
    let mut bytes = [0; 65536];
    while context.barrier.recorder().boundary().queued != 0 {
        assert!(Instant::now() < watchdog, "actual trace did not drain");
        match reader.read(&mut bytes) {
            Ok(0) => panic!("trace closed"),
            Ok(_) => {}
            Err(e) if e.kind() == std::io::ErrorKind::WouldBlock => std::thread::yield_now(),
            Err(e) => panic!("trace drain: {e}"),
        }
    }
}

pub(in crate::flow::context::startup_composition) fn drive(
    startup: StartupComposition<'_>,
    io: &mut FakePumpIo,
    reader: &mut UnixStream,
    state: &mut UnixStream,
    mode: &str,
) {
    let exchange = ExchangeId {
        opener: Opener::Admin,
        ordinal: 37,
    };
    let mut startup = if mode == "ready" {
        let (owner, error) = match startup.into_refusal(exchange) {
            Err(error) => error,
            Ok(_) => panic!("Ready became a refusal"),
        };
        assert!(matches!(error, RefusalJoinError::WrongStage));
        assert_eq!(owner.stage(), StartupStage::Complete);
        assert!(owner.classifier.is_some());
        assert!(owner.context.enter_refusal_record.is_none());
        return;
    } else {
        startup
    };
    let bound = startup.enter_bound.unwrap();
    let origin = startup.context.origin_ns();
    assert!(
        startup.context.flow.held_admission().is_none(),
        "actual result was already consumed by startup"
    );
    if mode == "foreign" {
        let (owner, error) = match startup.into_refusal(ExchangeId {
            opener: Opener::Harness,
            ordinal: 37,
        }) {
            Err(error) => error,
            Ok(_) => panic!("foreign exchange accepted"),
        };
        assert!(matches!(error, RefusalJoinError::WrongExchange));
        assert!(owner.context.enter_refusal_record.is_none());
        startup = owner;
    }
    if mode == "replaced" {
        startup.context.deadlines.disarm(PumpDeadline::Enter);
        startup
            .context
            .deadlines
            .arm(
                PumpDeadline::Enter,
                AbsoluteBound::after(bound.began_ns(), 2_000_000_000).unwrap(),
            )
            .unwrap();
        let (owner, error) = match startup.into_refusal(exchange) {
            Err(error) => error,
            Ok(_) => panic!("replacement Enter accepted"),
        };
        assert!(matches!(error, RefusalJoinError::WrongWindow));
        assert!(owner.context.enter_refusal_record.is_none());
        assert!(matches!(
            owner.stop,
            Some(StartupStop::ClassifierRefused(_))
        ));
        return;
    }
    let mut owner = match startup.into_refusal(exchange.clone()) {
        Ok(owner) => owner,
        Err((_, error)) => panic!("refusal join: {error:?}"),
    };
    let source_address = owner.source().detail.as_ptr();
    assert_eq!(owner.source().detail, "scripted startup refusal");
    assert_eq!(
        owner.source().code,
        s::Ste100AdmissionCode::UnsupportedArtifact
    );
    assert!(owner.recorded_sequence().is_none());
    owner = match owner.finish(&mut ports(io)) {
        Err(owner) => owner,
        Ok(_) => panic!("unrecorded answer exposed"),
    };
    if mode == "waiting" {
        owner.startup.context.pause_claim_queue_for_test = true;
        assert_eq!(
            owner.advance_record(&mut ports(io)),
            Err(RunFailure::EnterRefusalOwned)
        );
        assert!(owner.original.is_none());
        let waiting = owner.startup.context.waiting_content.as_ref().unwrap();
        assert_eq!(waiting.kind, Kind::Refusal);
        assert!(matches!(waiting.claim, Some(AuthorClaim::EnterRefusal(_))));
        assert!(owner.recorded_sequence().is_none());
        assert!(owner.startup.context.barrier.pending_author().is_none());
    }
    if mode == "pressure" || mode == "observer" {
        fill(owner.startup.context, io);
        let label = AbsoluteBound::after(io.now().unwrap(), 100).unwrap();
        owner
            .startup
            .context
            .arm(PumpDeadline::Label, label, io)
            .unwrap();
        let result = owner.advance_record(&mut ports(io));
        assert_eq!(
            result,
            Err(RunFailure::Wait {
                key: PumpDeadline::Label,
                bound: label
            })
        );
        let pending = owner.startup.context.barrier.pending_author().unwrap();
        assert_eq!(pending.event.kind, Kind::Refusal);
        assert_eq!(
            owner
                .startup
                .context
                .enter_refusal_record
                .as_ref()
                .unwrap()
                .ticket,
            Some(pending.ticket)
        );
        assert!(owner.recorded_sequence().is_none());
        assert_eq!(owner.source().detail.as_ptr(), source_address);
        owner
            .startup
            .context
            .complete_wait(PumpDeadline::Label)
            .unwrap();
        drain(owner.startup.context, reader);
        if mode == "observer" {
            owner
                .startup
                .context
                .barrier
                .fail_next_recorded_observer_for_test();
            let failure = owner.advance_record(&mut ports(io)).unwrap_err();
            let sequence = owner
                .recorded_sequence()
                .expect("exact Recorded witness retained before failed observer");
            assert_eq!(owner.startup.context.failure(), Some(&failure));
            assert_eq!(owner.startup.context.phase(), RecordingPhase::Failed);
            assert_eq!(
                owner
                    .startup
                    .context
                    .barrier
                    .recorded_author_ticket(sequence),
                owner
                    .startup
                    .context
                    .enter_refusal_record
                    .as_ref()
                    .unwrap()
                    .ticket
            );
            owner = match owner.finish(&mut ports(io)) {
                Err(owner) => owner,
                Ok(_) => panic!("Failed grants answer"),
            };
            assert_eq!(owner.source().detail.as_ptr(), source_address);
            assert_eq!(owner.recorded_sequence(), Some(sequence));
            assert!(owner.startup.resources.classifier_child().is_some());
            drain(owner.startup.context, reader);
            return;
        }
    }
    let sequence = owner.advance_record(&mut ports(io)).unwrap();
    assert_eq!(owner.recorded_sequence(), Some(sequence));
    assert_eq!(owner.advance_record(&mut ports(io)).unwrap(), sequence);
    assert_eq!(owner.startup.context.origin_ns(), origin);
    assert_eq!(
        owner.startup.context.deadlines.get(PumpDeadline::Enter),
        Some(bound)
    );
    assert_eq!(owner.startup.context.phase(), RecordingPhase::Entered);
    assert!(owner.startup.context.failure().is_none());
    let records: Vec<_> = owner
        .startup
        .context
        .barrier
        .recorder()
        .structure()
        .by_kind(Kind::Refusal)
        .collect();
    assert_eq!(
        records.len(),
        1,
        "one aggregate Enter refusal, including retries"
    );
    assert_eq!(records[0].sequence, sequence);
    let value: serde_json::Value = serde_json::from_str(&records[0].line).unwrap();
    let expected = serde_json::to_value(RefusalRecord::Lifecycle {
        asked: LifecycleAsk::Enter,
        refusal: match &owner.envelope.payload {
            weaver_types::Payload::Refusal(r) => r.clone(),
            _ => unreachable!(),
        },
    })
    .unwrap();
    assert_eq!(value["payload"], expected);
    if mode == "expiry" {
        io.advance_to(bound.deadline_ns()).unwrap();
        owner = match owner.finish(&mut ports(io)) {
            Err(owner) => owner,
            Ok(_) => panic!("expired Enter grants answer"),
        };
        assert_eq!(owner.recorded_sequence(), Some(sequence));
        assert_eq!(owner.source().detail.as_ptr(), source_address);
        drain(owner.startup.context, reader);
        return;
    }
    let recorded = match owner.finish(&mut ports(io)) {
        Ok(recorded) => recorded,
        Err(_) => panic!("recorded refusal did not finish"),
    };
    assert_eq!(recorded.sequence(), sequence);
    assert_eq!(recorded.source().detail.as_ptr(), source_address);
    assert_eq!(recorded.envelope().exchange, exchange);
    assert_eq!(recorded.envelope().position, weaver_types::Position::Close);
    assert!(
        matches!(&recorded.envelope().payload, weaver_types::Payload::Refusal(weaver_types::LifecycleRefusal::StateManagementInvalid { detail, .. }) if detail == "unsupported_artifact: scripted startup refusal")
    );
    assert!(recorded.owner.startup.opened.is_some());
    assert_eq!(recorded.owner.startup.load_sequence, Some(Sequence(0)));
    assert!(
        recorded
            .owner
            .startup
            .resources
            .classifier_child()
            .is_some()
    );
    assert!(recorded.owner.startup.resources.spu_child().is_none());
    if mode.starts_with("response-") {
        super::response::tests::drive(recorded, io, reader, state, mode);
        return;
    }
    drain(recorded.owner.startup.context, reader);
}

#[test]
fn enter_refusal_composition_records_actual_startup_result_once() {
    super::super::tests::refusal_join("record");
}
#[test]
fn enter_refusal_composition_preserves_unqueued_author() {
    super::super::tests::refusal_join("waiting");
}
#[test]
fn enter_refusal_composition_preserves_pending_ticket_under_pressure() {
    super::super::tests::refusal_join("pressure");
}
#[test]
fn enter_refusal_composition_retains_recorded_before_observer_failure() {
    super::super::tests::refusal_join("observer");
}
#[test]
fn enter_refusal_composition_rejects_ready() {
    super::super::tests::refusal_join("ready");
}
#[test]
fn enter_refusal_composition_rejects_foreign_exchange_without_losing_source() {
    super::super::tests::refusal_join("foreign");
}
#[test]
fn enter_refusal_composition_rejects_replacement_enter() {
    super::super::tests::refusal_join("replaced");
}
#[test]
fn enter_refusal_composition_expired_finish_keeps_recorded_refusal() {
    super::super::tests::refusal_join("expiry");
}
