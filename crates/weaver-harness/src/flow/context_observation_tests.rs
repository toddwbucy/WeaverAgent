//! Measurement wakes through the actual pending classifier and trace owners.

use super::*;
use crate::flow::clock::{AbsoluteBound, DeadlineRegistry};
use crate::flow::pump::{LifecyclePeer, PumpDeadline, PumpSource, PumpStep};
use nix::sys::socket::{MsgFlags, send};
use std::os::fd::AsRawFd;
use weaver_types::{ExchangeId, LifecycleAnswer, Opener, OrganEnvelope, Position};

fn pending_check() -> InputFixture {
    let mut fixture = InputFixture::new();
    fixture.begin(TurnFrame::carry_from(br#"{"text":"Read item."}"#, 42));
    let request = fixture.request();
    assert!(matches!(request.target, s::Ste100Target::Input { .. }));
    for _ in 0..16 {
        if fixture.advance().unwrap() == InputProgress::Pending {
            break;
        }
    }
    assert!(fixture.run.context.flow.pending_check().is_some());
    assert!(fixture.records(Kind::Ste100Result).is_empty());
    assert!(next_tick(&mut fixture).is_some());
    fixture
}

fn next_tick(fixture: &mut InputFixture) -> Option<u64> {
    let context = &mut fixture.run.context;
    context
        .flow
        .observation_wake(&mut FlowRecording {
            barrier: &mut context.barrier,
            inbox: &mut context.inbox,
            budget: &mut context.budget,
            author: &context.author,
        })
        .unwrap()
}

fn pump(fixture: &mut InputFixture, deadlines: &DeadlineRegistry<PumpDeadline>) -> PumpStep {
    let context = &mut fixture.run.context;
    context
        .pump
        .step(
            &mut ports(&mut fixture.run.clock),
            Some(&mut context.flow),
            &mut FlowRecording {
                barrier: &mut context.barrier,
                inbox: &mut context.inbox,
                budget: &mut context.budget,
                author: &context.author,
            },
            deadlines,
        )
        .unwrap()
}

fn samples(fixture: &InputFixture) -> Vec<serde_json::Value> {
    fixture
        .records(Kind::StateFlow)
        .into_iter()
        .filter(|record| record["payload"]["phase"] == "sampled")
        .collect()
}

fn record_sample(fixture: &mut InputFixture, count: usize) {
    for _ in 0..16 {
        fixture
            .run
            .context
            .reduce_flow(&mut fixture.run.clock)
            .unwrap();
        if samples(fixture).len() == count {
            return;
        }
    }
    panic!("the observation wake did not produce its recorded sample");
}

#[test]
fn silent_input_check_wakes_and_records_zero_receipts_without_an_operation_deadline() {
    let mut fixture = pending_check();
    let gate = fixture.run.context.deadlines.get(PumpDeadline::Gate);
    let tick = next_tick(&mut fixture).unwrap();
    let deadlines = DeadlineRegistry::new();
    assert_eq!(pump(&mut fixture, &deadlines), PumpStep::Observation);
    assert_eq!(fixture.run.clock.now().unwrap(), tick);
    assert_eq!(
        fixture.run.clock.waits().last().unwrap().absolute_deadline,
        Some(tick)
    );
    assert_eq!(fixture.run.context.deadlines.get(PumpDeadline::Gate), gate);
    record_sample(&mut fixture, 1);
    let records = samples(&fixture);
    assert_eq!(records[0]["payload"]["sampled_ns"], tick.to_string());
    assert_eq!(records[0]["payload"]["filed_count"], "0");
    assert_eq!(records[0]["payload"]["filed_bytes"], "0");
    assert_eq!(records[0]["payload"]["input_pending"], "1");
    assert_eq!(records[0]["payload"]["input_completed"], "0");
    assert!(fixture.records(Kind::Ste100Result).is_empty());
    assert!(fixture.run.context.halt().is_none());
    fixture.no_request();
}

#[test]
fn observation_wake_preserves_the_later_operation_deadline_and_next_sample_interval() {
    let mut fixture = pending_check();
    let gate = fixture
        .run
        .context
        .deadlines
        .get(PumpDeadline::Gate)
        .unwrap();
    let tick = next_tick(&mut fixture).unwrap();
    assert!(tick < gate.deadline_ns());
    assert_eq!(
        fixture
            .run
            .context
            .pump_once(&mut ports(&mut fixture.run.clock))
            .unwrap(),
        PumpStep::Observation
    );
    record_sample(&mut fixture, 1);
    assert_eq!(
        fixture.run.context.deadlines.get(PumpDeadline::Gate),
        Some(gate)
    );
    let interval = fixture
        .run
        .context
        .declaration
        .observation_interval
        .elapsed_ns
        .0;
    assert_eq!(next_tick(&mut fixture), Some(tick + interval));
    assert!(fixture.run.context.halt().is_none());
}

#[test]
fn ready_lifecycle_descriptor_preserves_sampling_at_or_after_the_observation_tick() {
    for late in [false, true] {
        let mut fixture = pending_check();
        let gate = fixture.run.context.deadlines.get(PumpDeadline::Gate);
        let tick = next_tick(&mut fixture).unwrap();
        let interval = fixture
            .run
            .context
            .declaration
            .observation_interval
            .elapsed_ns
            .0;
        let now = tick + u64::from(late) * interval;
        fixture.run.clock.advance_to(now).unwrap();
        let waits = fixture.run.clock.waits().len();
        let (end, peer) = socketpair(
            AddressFamily::Unix,
            SockType::SeqPacket,
            None,
            SockFlag::SOCK_CLOEXEC,
        )
        .unwrap();
        let mut lifecycle = LifecyclePeer::from_owned(end).unwrap();
        let envelope = OrganEnvelope {
            exchange: ExchangeId {
                opener: Opener::Admin,
                ordinal: 1,
            },
            position: Position::Open,
            payload: weaver_types::Payload::Answer(LifecycleAnswer::Ready),
        };
        let bytes = serde_json::to_vec(&envelope).unwrap();
        assert_eq!(
            send(
                peer.as_raw_fd(),
                &bytes,
                MsgFlags::MSG_DONTWAIT | MsgFlags::MSG_NOSIGNAL,
            )
            .unwrap(),
            bytes.len()
        );
        let mut active_ports = ports(&mut fixture.run.clock);
        active_ports.lifecycle.peer = Some(&mut lifecycle);
        let step = fixture.run.context.pump_once(&mut active_ports).unwrap();
        assert!(
            matches!(step, PumpStep::Progress { sources } if sources.contains(&PumpSource::Admin))
        );
        assert!(lifecycle.held().is_some());
        assert_eq!(fixture.run.clock.waits().len(), waits);
        assert!(samples(&fixture).is_empty());
        record_sample(&mut fixture, 1);
        let records = samples(&fixture);
        assert_eq!(records[0]["payload"]["sampled_ns"], now.to_string());
        assert_eq!(records[0]["payload"]["filed_count"], "0");
        assert_eq!(records[0]["payload"]["input_pending"], "1");
        assert_eq!(fixture.run.context.deadlines.get(PumpDeadline::Gate), gate);
        assert_eq!(next_tick(&mut fixture), Some(now + interval));
        assert!(fixture.run.context.flow.pending_check().is_some());
        assert!(fixture.run.context.halt().is_none());
    }
}

#[test]
fn terminal_deadline_wins_before_or_at_the_observation_tick() {
    for before in [true, false] {
        let mut fixture = pending_check();
        let tick = next_tick(&mut fixture).unwrap();
        let end = tick - u64::from(before);
        let bound = AbsoluteBound::after(
            fixture.run.clock.now().unwrap(),
            end - fixture.run.clock.now().unwrap(),
        )
        .unwrap();
        let mut deadlines = DeadlineRegistry::new();
        deadlines.arm(PumpDeadline::Decision, bound).unwrap();
        assert_eq!(
            pump(&mut fixture, &deadlines),
            PumpStep::Deadline {
                key: PumpDeadline::Decision,
                bound,
                observed_ns: end,
            }
        );
        assert!(samples(&fixture).is_empty());
        assert_eq!(deadlines.get(PumpDeadline::Decision), Some(bound));
    }
}

#[test]
fn idle_flow_does_not_arm_an_observation_wake() {
    let mut fixture = InputFixture::new();
    assert_eq!(next_tick(&mut fixture), None);
    let now = fixture.run.clock.now().unwrap();
    let bound = AbsoluteBound::after(now, 5000).unwrap();
    let mut deadlines = DeadlineRegistry::new();
    deadlines.arm(PumpDeadline::Leave, bound).unwrap();
    assert_eq!(
        pump(&mut fixture, &deadlines),
        PumpStep::Deadline {
            key: PumpDeadline::Leave,
            bound,
            observed_ns: bound.deadline_ns(),
        }
    );
    assert_eq!(
        fixture.run.clock.waits().last().unwrap().absolute_deadline,
        Some(bound.deadline_ns())
    );
    assert!(samples(&fixture).is_empty());
}

#[test]
fn trace_pressure_coalesces_ticks_behind_the_original_pending_sample() {
    let mut fixture = pending_check();
    let first_tick = next_tick(&mut fixture).unwrap();
    fixture.fill_trace();
    fixture.run.clock.advance_to(first_tick).unwrap();
    // Capture one sample, then retain that exact sample's author under real
    // trace backpressure. Advancing time must not create a second author.
    fixture
        .run
        .context
        .reduce_flow(&mut fixture.run.clock)
        .unwrap();
    fixture
        .run
        .context
        .reduce_flow(&mut fixture.run.clock)
        .unwrap();
    assert!(fixture.run.context.flow.has_pending_record());
    let author = format!(
        "{:?}",
        fixture.run.context.barrier.pending_author().unwrap()
    );
    assert_eq!(next_tick(&mut fixture), None);
    let interval = fixture
        .run
        .context
        .declaration
        .observation_interval
        .elapsed_ns
        .0;
    let bound = AbsoluteBound::after(first_tick, interval * 4).unwrap();
    let mut deadlines = DeadlineRegistry::new();
    deadlines.arm(PumpDeadline::Decision, bound).unwrap();
    assert_eq!(
        pump(&mut fixture, &deadlines),
        PumpStep::Deadline {
            key: PumpDeadline::Decision,
            bound,
            observed_ns: bound.deadline_ns(),
        }
    );
    assert_eq!(
        fixture.run.clock.waits().last().unwrap().absolute_deadline,
        Some(bound.deadline_ns())
    );
    assert_eq!(
        format!(
            "{:?}",
            fixture.run.context.barrier.pending_author().unwrap()
        ),
        author
    );
    assert!(samples(&fixture).is_empty());
    fixture.drain_trace();
    record_sample(&mut fixture, 1);
    record_sample(&mut fixture, 2);
    let records = samples(&fixture);
    assert_eq!(records[0]["payload"]["sampled_ns"], first_tick.to_string());
    assert_eq!(
        records[1]["payload"]["sample_start_ns"],
        first_tick.to_string()
    );
    assert_eq!(
        records[1]["payload"]["sampled_ns"],
        bound.deadline_ns().to_string()
    );
    assert_eq!(records[1]["payload"]["filed_count"], "0");
    assert_eq!(
        next_tick(&mut fixture),
        Some(bound.deadline_ns() + interval)
    );
}

#[test]
fn observation_wake_overflow_is_a_typed_flow_failure_before_waiting() {
    let mut declaration = config();
    declaration.observation_interval.elapsed_ns = u64::MAX.into();
    let mut fixture = InputFixture::with_declaration(None, declaration);
    fixture.begin(TurnFrame::carry_from(br#"{"text":"Read item."}"#, 42));
    let context = &mut fixture.run.context;
    let error = context
        .flow
        .observation_wake(&mut FlowRecording {
            barrier: &mut context.barrier,
            inbox: &mut context.inbox,
            budget: &mut context.budget,
            author: &context.author,
        })
        .unwrap_err();
    assert_eq!(error.point, "observation");
    assert!(error.detail.contains("CounterOverflow"));
    assert!(context.flow.failure().is_some());
}
