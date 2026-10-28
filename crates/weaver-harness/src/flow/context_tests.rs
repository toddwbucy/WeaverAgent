use super::super::author::AuthoredSource;
use super::super::clock::{ReadyFd, testing::FakePumpIo};
use super::super::coordinator::CoordinatorPhase;
use super::super::pump::LifecyclePorts;
use super::super::recording_fixture as fixture;
use super::*;
use nix::poll::PollFlags;
use std::io::{BufRead, BufReader, Write};
use std::os::unix::net::UnixStream;
use std::time::Duration;
use weaver_trace::{Subsystem, TurnRef};
use weaver_types::state_flow as s;

pub(super) struct Fixture {
    pub(super) context: RecordingContext,
    pub(super) clock: FakePumpIo,
    pub(super) state: UnixStream,
    pub(super) _reader: UnixStream,
}

pub(super) fn config() -> FlowConfig {
    let load = fixture::load_record();
    let digest = s::Digest::new("a".repeat(64)).unwrap();
    FlowConfig {
        version: 1,
        partition: fixture::partition()
            .rules
            .into_iter()
            .map(|rule| s::PartitionRule {
                id: rule.id,
                kind: rule.kind,
                channels: rule.channels,
                origins: rule.origins,
                blocks: rule.blocks,
                context: rule.context,
            })
            .collect(),
        gate_wait: serde_json::from_value(serde_json::to_value(load.state_flow.gate_wait).unwrap())
            .unwrap(),
        cut_wait: serde_json::from_value(serde_json::to_value(load.state_flow.cut_wait).unwrap())
            .unwrap(),
        observation_interval: s::MeasuredInterval {
            elapsed_ns: 1000.into(),
            measurement_ref: "fixture".into(),
        },
        backlog: s::FlowBacklog {
            max_pending_blocks: 100.into(),
            max_pending_bytes: 1000000.into(),
            lag_alarm_turns: 2.into(),
            measurement_ref: "fixture".into(),
        },
        scheme: s::SchemeDeclaration {
            scheme_id: format!("ste100:{}", digest.as_str()),
            artifact_sha256: digest,
            verbs: vec!["read".into()],
        },
    }
}

pub(super) fn event(kind: Kind, payload: Option<Payload>) -> AuthoredEvent {
    AuthoredEvent {
        kind,
        subsystem: Subsystem::Harness,
        turn: None,
        source: AuthoredSource::Runtime,
        causal_parent: None,
        payload,
    }
}

impl Fixture {
    pub(super) fn new(declaration: FlowConfig) -> Self {
        Self::with_enter(declaration, 1_000_000_000)
    }

    pub(super) fn with_enter(declaration: FlowConfig, remaining_enter_ns: u64) -> Self {
        let (sink, reader) = UnixStream::pair().unwrap();
        let (state, peer) = UnixStream::pair().unwrap();
        peer.set_read_timeout(Some(Duration::from_secs(3))).unwrap();
        peer.set_write_timeout(Some(Duration::from_secs(3)))
            .unwrap();
        let mut clock = FakePumpIo::new(100);
        let context = RecordingContext::new(
            sink.into(),
            StateChannel::new(state, "session".into(), "run".into()).unwrap(),
            "session".into(),
            "run".into(),
            42,
            declaration,
            AbsoluteBound::after(100, remaining_enter_ns).unwrap(),
            &mut clock,
        )
        .unwrap();
        Self {
            context,
            clock,
            state: peer,
            _reader: reader,
        }
    }

    pub(super) fn pump_at(&mut self, time: u64, flags: PollFlags) {
        self.clock
            .schedule(
                time,
                vec![ReadyFd {
                    index: 0,
                    events: flags,
                }],
            )
            .unwrap();
        self.context
            .pump_once(&mut PumpPorts {
                lifecycle: LifecyclePorts::default(),
                gate: None,
                decode: None,
                label: None,
                io: &mut self.clock,
            })
            .unwrap();
    }

    pub(super) fn opened(&mut self) {
        let start: StartIdentity = serde_json::from_value(
            serde_json::to_value(fixture::load_record().state_start).unwrap(),
        )
        .unwrap();
        self.context
            .begin_open(
                start,
                ReadLimits {
                    max_result_bytes: 100000.into(),
                    measurement_ref: "fixture".into(),
                },
                &mut self.clock,
            )
            .unwrap();
        self.pump_at(101, PollFlags::POLLOUT);
        let mut line = Vec::new();
        BufReader::new(&mut self.state)
            .read_until(b'\n', &mut line)
            .unwrap();
        let request = s::decode_state_line(&line, s::FrameDirection::Request).unwrap();
        let observation = s::StateOperationObservation {
            operation_id: 1.into(),
            parent_id: None,
            request_id: request.common().request_id,
            operation: s::StateOperation::Open,
            began_ns: 102.into(),
            ended_ns: 103.into(),
            phases: vec![
                s::OperationPhase {
                    name: s::PhaseName::Validate,
                    began_ns: 102.into(),
                    ended_ns: 102.into(),
                },
                s::OperationPhase {
                    name: s::PhaseName::Encode,
                    began_ns: 103.into(),
                    ended_ns: 103.into(),
                },
            ],
            resource: s::OperationResource {
                cpu_ns: None,
                copied_bytes: Some(0.into()),
                held_bytes: Some(0.into()),
                unavailable: vec![s::ResourceUnavailable {
                    field: s::ResourceField::CpuNs,
                    reason: s::ObservationUnavailable::NotSampled,
                }],
            },
            disk: s::OperationDisk {
                enabled: None,
                before: None,
                after: None,
                unavailable_reason: Some(s::ObservationUnavailable::NotSampled),
            },
        };
        let s::StateFrame::Open {
            common,
            schema_id,
            scheme_id,
            start,
            read_limits,
        } = request
        else {
            panic!("open");
        };
        let response = s::StateFrame::Opened {
            common,
            schema_id,
            scheme_id,
            start,
            read_limits,
            observation,
            member_observation: s::MemberObservationIdentity {
                instrument_version: "state-observation/2".into(),
                pid: 7,
                start_ticks: Some(10.into()),
                ticks_per_second: Some(100.into()),
                unavailable_reason: None,
            },
        };
        self.state
            .write_all(&s::encode_state_line(&response, s::FrameDirection::Response).unwrap())
            .unwrap();
        self.pump_at(104, PollFlags::POLLIN);
        assert!(self.context.opened_identity().is_some());
    }

    pub(super) fn load_event(&self) -> AuthoredEvent {
        let mut load = fixture::load_record();
        load.origin_ns = self.context.origin_ns().into();
        load.state_flow.partition = self
            .context
            .declaration
            .partition
            .iter()
            .map(|r| serving::payloads::PartitionRule {
                id: r.id.clone(),
                kind: r.kind.clone(),
                channels: r.channels.clone(),
                origins: r.origins.clone(),
                blocks: r.blocks.clone(),
                context: r.context.clone(),
            })
            .collect();
        let declared = &self.context.declaration;
        load.state_flow.observation_interval.elapsed_ns =
            declared.observation_interval.elapsed_ns.0.into();
        load.state_flow.observation_interval.measurement_ref =
            declared.observation_interval.measurement_ref.clone();
        load.state_flow.backlog = serving::payloads::FlowBacklog {
            max_pending_blocks: declared.backlog.max_pending_blocks.0.into(),
            max_pending_bytes: declared.backlog.max_pending_bytes.0.into(),
            lag_alarm_turns: declared.backlog.lag_alarm_turns.0.into(),
            measurement_ref: declared.backlog.measurement_ref.clone(),
        };
        load.state_flow.reads.max_result_bytes = 100000.into();
        load.state_flow.reads.measurement_ref = "fixture".into();
        load.state_flow.scheme.verbs = self.context.declaration.scheme.verbs.clone();
        load.state_flow.scheme.scheme_id = self.context.declaration.scheme.scheme_id.clone();
        load.state_flow.scheme.artifact_sha256 = self
            .context
            .declaration
            .scheme
            .artifact_sha256
            .as_str()
            .into();
        load.template_inventory.scheme_id = declared.scheme.scheme_id.clone();
        load.template_inventory.checker_artifact_sha256 =
            load.state_flow.ste100.model_artifact_sha256.clone();
        load.state_member_observation =
            super::super::mirror::member(&s::MemberObservationIdentity {
                instrument_version: "state-observation/2".into(),
                pid: 7,
                start_ticks: Some(10.into()),
                ticks_per_second: Some(100.into()),
                unavailable_reason: None,
            });
        event(Kind::Load, Some(Payload::Elections(Box::new(load))))
    }

    pub(super) fn author(&mut self, event: AuthoredEvent) -> Result<Sequence, RunFailure> {
        self.context
            .author_and_wait(
                &mut PumpPorts {
                    lifecycle: LifecyclePorts::default(),
                    gate: None,
                    decode: None,
                    label: None,
                    io: &mut self.clock,
                },
                event,
            )
            .map_err(|failure| failure.retained_reason())
    }

    pub(super) fn activate(&mut self) {
        self.opened();
        assert_eq!(self.author(self.load_event()).unwrap(), Sequence(0));
        self.context.activate(&mut self.clock).unwrap();
        for _ in 0..10 {
            self.context.reduce_flow(&mut self.clock).unwrap();
            if self.context.flow.operation_completion().is_some() {
                break;
            }
        }
        assert!(matches!(
            self.context
                .take_operation_completion(&mut self.clock)
                .unwrap(),
            Some(OperationCompletion::Opened { .. })
        ));
    }
}

impl Drop for Fixture {
    fn drop(&mut self) {
        self.context.barrier.abandon();
    }
}

#[test]
fn the_same_context_carries_load_ticket_origin_and_open_proof_into_entered() {
    let mut fx = Fixture::new(config());
    let origin = fx.context.origin_ns();
    fx.activate();
    assert_eq!(fx.context.phase(), RecordingPhase::Entered);
    assert_eq!(fx.context.origin_ns(), origin);
    assert_eq!(fx.context.flow.phase(), CoordinatorPhase::Active);
    assert_eq!(fx.context.budget.gauges(), (0, 0));
    assert!(fx.context.flow.ledger().facts().next().is_none());
    assert!(fx.context.pending_author().is_none());
    assert!(
        fx.context
            .take_operation_completion(&mut fx.clock)
            .unwrap()
            .is_none()
    );
    let records = fx.context.barrier.recorder().structure();
    assert_eq!(records.len(), 2);
}

#[test]
fn load_needs_this_contexts_open_and_common_origin() {
    for open in [false, true] {
        let mut fx = Fixture::new(config());
        if open {
            fx.opened();
        }
        let mut load = fx.load_event();
        if open {
            let Some(Payload::Elections(record)) = &mut load.payload else {
                unreachable!()
            };
            record.origin_ns = (fx.context.origin_ns() + 1).into();
        }
        assert!(fx.author(load).is_err());
        assert_eq!(fx.context.phase(), RecordingPhase::Failed);
        assert!(!fx.context.barrier.is_loaded());
        assert!(fx.context.refused_author().is_some());
        assert!(fx.context.flow.halt().is_some());
    }
}

#[test]
fn invalid_partition_refuses_activation_without_a_component_halt() {
    let mut declaration = config();
    declaration.partition[0].kind = "save_point".into();
    let mut fx = Fixture::new(declaration);
    fx.opened();
    fx.author(fx.load_event()).unwrap();
    assert!(matches!(
        fx.context.activate(&mut fx.clock),
        Err(ActivationFailure::Partition(_))
    ));
    assert!(fx.context.failure().is_none());
    assert!(!fx.context.barrier.is_active());
    assert!(fx.context.flow.halt().is_none());
}

#[test]
fn a_second_activation_never_rebuilds_the_tap_or_reset_the_author() {
    let mut fx = Fixture::new(config());
    fx.activate();
    assert!(fx.context.activate(&mut fx.clock).is_err());
    assert!(fx.context.barrier.recorder().tap_stopped());
    let count = fx.context.barrier.recorder().structure().len();
    let content = AuthoredEvent { kind: Kind::MessageAssistant, subsystem: Subsystem::Harness,
        turn: Some(TurnRef("turn".into())), source: AuthoredSource::Model,
        causal_parent: None, payload: Some(Payload::Message(serde_json::value::RawValue::from_string(
            "{\"role\":\"assistant\",\"content\":[{\"type\":\"text\",\"text\":\"Read file.\"}]}".into()).unwrap())) };
    assert!(fx.author(content).is_err());
    assert_eq!(fx.context.barrier.recorder().structure().len(), count);
    assert!(fx.context.flow.ledger().facts().next().is_none());
}

#[test]
fn unload_cannot_close_an_entered_store_or_leave_an_owed_block_behind() {
    let mut fx = Fixture::new(config());
    fx.activate();
    let unload = event(
        Kind::Unload,
        Some(Payload::Unload(Box::new(unload_close()))),
    );
    assert!(fx.author(unload).is_err());
    assert!(!fx.context.barrier.is_closed());
    assert!(fx.context.flow.standing_state().is_some());
    assert_eq!(fx.context.phase(), RecordingPhase::Failed);
}

fn unload_close() -> serving::payloads::UnloadClose {
    serving::payloads::UnloadClose {
        grant_surface: None,
        cause: None,
        forced: false,
        forced_by: None,
        release: serving::payloads::Release {
            spu: None,
            member: None,
        },
        end_reason: None,
        state_flow_failure: None,
    }
}

#[test]
fn clean_release_retains_the_context_through_unload_then_never_reopens() {
    let mut fx = Fixture::new(config());
    fx.activate();
    fx.context.complete_wait(PumpDeadline::Enter).unwrap();
    let leave = AbsoluteBound::after(104, 10_000).unwrap();
    fx.context.begin_releasing(leave, &mut fx.clock).unwrap();
    assert_eq!(
        fx.context.retire_state(&mut fx.clock).unwrap(),
        Advance::Progress
    );
    assert_eq!(
        fx.context.retire_classifier(&mut fx.clock).unwrap(),
        Advance::Progress
    );
    assert!(fx.context.flow.standing_state().is_none());
    assert_eq!(
        fx.author(event(
            Kind::Unload,
            Some(Payload::Unload(Box::new(unload_close())))
        ))
        .unwrap(),
        Sequence(2)
    );
    assert!(fx.context.barrier.is_closed());
    // Synchronize the real writer before the fake wait lane is used below.
    // The production drain remains bounded by the common pump's leave window.
    let end = std::time::Instant::now() + Duration::from_secs(3);
    loop {
        if matches!(
            fx.context
                .barrier
                .drain_status(&mut fx.context.budget)
                .unwrap(),
            serving::DrainStatus::Emptied
        ) {
            break;
        }
        assert!(std::time::Instant::now() < end, "writer did not drain");
        std::thread::yield_now();
    }
    fx.context
        .close_after_drain(&mut PumpPorts {
            lifecycle: LifecyclePorts::default(),
            gate: None,
            decode: None,
            label: None,
            io: &mut fx.clock,
        })
        .unwrap();
    assert_eq!(fx.context.phase(), RecordingPhase::Closed);
    assert_eq!(
        fx.author(event(Kind::SessionClosed, None)).unwrap_err(),
        RunFailure::Closed
    );
    assert_eq!(fx.context.phase(), RecordingPhase::Closed);
    assert!(fx.context.flow.halt().is_none());
}

#[test]
fn lifecycle_deadlines_keep_their_owner_while_gate_expiry_latches_flow() {
    for key in [
        PumpDeadline::Enter,
        PumpDeadline::Leave,
        PumpDeadline::Lower,
        PumpDeadline::Label,
        PumpDeadline::Gate,
    ] {
        let mut fx = Fixture::new(config());
        fx.activate();
        fx.context.complete_wait(PumpDeadline::Enter).unwrap();
        let bound = AbsoluteBound::after(104, 5).unwrap();
        fx.context.arm(key, bound, &mut fx.clock).unwrap();
        fx.clock.advance_to(109).unwrap();
        let result = fx.author(event(Kind::SessionClosed, None)).unwrap_err();
        if key == PumpDeadline::Gate {
            assert!(matches!(
                result,
                RunFailure::Flow {
                    reason: FlowHaltReason::GateBound,
                    ..
                }
            ));
            assert_eq!(fx.context.phase(), RecordingPhase::Failed);
            assert_eq!(fx.context.failed_wait.as_ref().unwrap().deadline_ns.0, 109);
        } else {
            assert_eq!(result, RunFailure::Wait { key, bound });
            assert_eq!(fx.context.phase(), RecordingPhase::Entered);
            assert!(fx.context.flow.halt().is_none());
        }
        assert_eq!(fx.context.barrier.recorder().structure().len(), 2);
    }
}

#[derive(Clone, Copy)]
enum TimeoutClock {
    Elapsed(u64),
    Failed,
    Reversed,
}

struct CrossDeadlineWait {
    before: u64,
    deadline: u64,
    after: TimeoutClock,
    waited: bool,
    after_reads: usize,
}

impl PumpIo for CrossDeadlineWait {
    fn now(&mut self) -> std::io::Result<u64> {
        if !self.waited {
            return Ok(self.before);
        }
        self.after_reads += 1;
        match self.after {
            TimeoutClock::Elapsed(observed) => {
                assert_eq!(
                    self.after_reads, 1,
                    "retain the observation without resampling"
                );
                Ok(observed)
            }
            TimeoutClock::Failed => Err(std::io::Error::other("post-wait clock failed")),
            TimeoutClock::Reversed => Ok(self.before - 1),
        }
    }

    fn wait(
        &mut self,
        _: &[crate::flow::clock::WaitInterest<'_>],
        deadline: Option<u64>,
    ) -> std::io::Result<crate::flow::clock::WaitOutcome> {
        assert!(!self.waited, "one wait crosses the retained bound");
        assert_eq!(deadline, Some(self.deadline));
        self.waited = true;
        Ok(crate::flow::clock::WaitOutcome::Timeout)
    }
}

#[test]
fn pump_wait_expiry_retains_actual_time_and_original_lifecycle_bound() {
    for key in [PumpDeadline::Enter, PumpDeadline::Leave] {
        let mut fx = Fixture::with_enter(config(), 5);
        let bound = fx.context.deadlines.get(PumpDeadline::Enter).unwrap();
        if key == PumpDeadline::Leave {
            fx.context.complete_wait(PumpDeadline::Enter).unwrap();
            fx.context.arm(key, bound, &mut fx.clock).unwrap();
        }
        let observed = bound.deadline_ns() + 7;
        let mut io = CrossDeadlineWait {
            before: fx.context.last_ns,
            deadline: bound.deadline_ns(),
            after: TimeoutClock::Elapsed(observed),
            waited: false,
            after_reads: 0,
        };
        let result = fx.context.pump_once(&mut author_ports(&mut io));
        assert!(
            io.waited,
            "expiry must first be observed after the actual wait"
        );
        assert_eq!(io.after_reads, 1);
        assert_eq!(result, Err(RunFailure::Wait { key, bound }));
        assert_eq!(
            fx.context.last_ns, observed,
            "retain actual time, not the bound"
        );
        assert_eq!(fx.context.deadlines.get(key), Some(bound));
        assert_eq!(fx.context.phase(), RecordingPhase::Preflight);
        assert!(fx.context.failure().is_none());
        assert!(fx.context.flow.halt().is_none());
    }
}

#[test]
fn pump_wait_clock_failure_does_not_become_healthy_lifecycle_expiry() {
    for after in [TimeoutClock::Failed, TimeoutClock::Reversed] {
        let mut fx = Fixture::with_enter(config(), 5);
        let bound = fx.context.deadlines.get(PumpDeadline::Enter).unwrap();
        let before = fx.context.last_ns;
        let mut io = CrossDeadlineWait {
            before,
            deadline: bound.deadline_ns(),
            after,
            waited: false,
            after_reads: 0,
        };
        let result = fx.context.pump_once(&mut author_ports(&mut io));
        assert!(io.waited);
        assert!(matches!(
            result,
            Err(RunFailure::Flow {
                reason: FlowHaltReason::TraceFailed,
                point: HaltPoint::Author,
                ..
            })
        ));
        assert_eq!(fx.context.failure(), result.as_ref().err());
        assert_eq!(fx.context.last_ns, before);
        assert_eq!(fx.context.deadlines.get(PumpDeadline::Enter), Some(bound));
        assert_eq!(fx.context.phase(), RecordingPhase::Failed);
    }
}

pub(in crate::flow) fn with_frozen_source<T>(
    use_facts: impl FnOnce(
        &FlowCoordinator<SocketPort>,
        &RecordBarrier,
        &FlowInbox,
        &RetentionBudget,
    ) -> T,
) -> T {
    let mut fx = Fixture::new(config());
    fx.activate();
    fx.author(AuthoredEvent { kind: Kind::MessageAssistant, subsystem: Subsystem::Harness,
        turn: Some(TurnRef("turn".into())), source: AuthoredSource::Model, causal_parent: None,
        payload: Some(Payload::Message(serde_json::value::RawValue::from_string(
            "{\"role\":\"assistant\",\"content\":[{\"type\":\"text\",\"text\":\"Read file.\"}]}".into()).unwrap())) }).unwrap();
    fx.context.fail(
        &mut fx.clock,
        FlowHaltReason::TraceFailed,
        HaltPoint::Author,
        "fixture failure after admission",
    );
    use_facts(
        &fx.context.flow,
        &fx.context.barrier,
        &fx.context.inbox,
        &fx.context.budget,
    )
}

fn author_ports<I: PumpIo>(io: &mut I) -> PumpPorts<'_, I> {
    PumpPorts {
        lifecycle: LifecyclePorts::default(),
        gate: None,
        decode: None,
        label: None,
        io,
    }
}

#[test]
fn deferred_author_rechecks_phase_after_a_real_release_transition() {
    let mut fx = Fixture::new(config());
    fx.activate();
    fx.context.complete_wait(PumpDeadline::Enter).unwrap();
    let now = fx.clock.now().unwrap();
    let leave = AbsoluteBound::after(now, 5).unwrap();
    fx.context
        .arm(PumpDeadline::Leave, leave, &mut fx.clock)
        .unwrap();
    fx.clock.advance_to(leave.deadline_ns()).unwrap();
    let mut original = event(
        Kind::ModelMeasurement,
        Some(Payload::ModelMeasurement(
            serde_json::value::to_raw_value(&serde_json::json!({"fixture":"deferred measurement"}))
                .unwrap(),
        )),
    );
    original.turn = Some(TurnRef("measurement-turn".into()));
    assert_eq!(
        fx.author(original.clone()).unwrap_err(),
        RunFailure::Wait {
            key: PumpDeadline::Leave,
            bound: leave
        }
    );
    assert!(fx.context.author_admission_pending);
    assert!(fx.context.pending_author().is_none());
    assert!(fx.context.refused_author.is_none());
    assert_eq!(fx.context.complete_wait(PumpDeadline::Leave), Some(leave));
    let releasing = AbsoluteBound::after(leave.deadline_ns(), 100).unwrap();
    fx.context
        .begin_releasing(releasing, &mut fx.clock)
        .unwrap();
    assert_eq!(fx.context.phase(), RecordingPhase::Releasing);
    assert!(matches!(
        fx.context
            .resume_author_and_wait(&mut author_ports(&mut fx.clock))
            .map_err(|failure| failure.retained_reason()),
        Err(RunFailure::Flow {
            reason: FlowHaltReason::ProtocolFailed,
            point: HaltPoint::Author,
            ..
        })
    ));
    assert_eq!(
        format!("{:?}", fx.context.refused_author.as_ref().unwrap()),
        format!("{original:?}")
    );
    assert!(fx.context.waiting_content.is_none());
    assert!(!fx.context.author_admission_pending);
    assert_eq!(
        fx.context
            .barrier
            .recorder()
            .structure()
            .by_kind(Kind::ModelMeasurement)
            .count(),
        0
    );
}

#[test]
fn initial_author_wait_never_promotes_an_invalid_original() {
    for kind in [Kind::ToolCallStarted, Kind::Load] {
        let mut fx = Fixture::new(config());
        fx.activate();
        fx.context.complete_wait(PumpDeadline::Enter).unwrap();
        let now = fx.clock.now().unwrap();
        let leave = AbsoluteBound::after(now, 5).unwrap();
        fx.context
            .arm(PumpDeadline::Leave, leave, &mut fx.clock)
            .unwrap();
        fx.clock.advance_to(leave.deadline_ns()).unwrap();
        let original = event(kind, None);
        let count = fx.context.barrier.recorder().structure().len();
        let error = fx.author(original.clone()).unwrap_err();
        if kind == Kind::ToolCallStarted {
            assert_eq!(error, RunFailure::PairOwned);
        } else {
            assert!(matches!(
                error,
                RunFailure::Flow {
                    reason: FlowHaltReason::ProtocolFailed,
                    point: HaltPoint::Author,
                    ..
                }
            ));
        }
        assert_eq!(
            format!(
                "{:?}",
                if kind == Kind::ToolCallStarted {
                    fx.context.held_author().unwrap()
                } else {
                    fx.context.refused_author().unwrap()
                }
            ),
            format!("{original:?}")
        );
        assert!(fx.context.waiting_content.is_none());
        assert!(!fx.context.author_admission_pending);
        assert!(fx.context.pending_author().is_none());
        assert_eq!(fx.context.barrier.recorder().structure().len(), count);
    }
}

#[test]
fn initial_author_wait_keeps_template_refusal_and_fatal_liveness_distinct() {
    use crate::flow::templates::CompiledInventory;
    for fatal in [false, true] {
        let templates = Box::leak(
            vec![s::TemplateSpec {
                template_id: "fixture.deferred-author".into(),
                kind: "loop.instruction".into(),
                parts: vec![s::TemplatePart::Literal("Read file.".into())],
            }]
            .into_boxed_slice(),
        );
        let inventory = CompiledInventory::new(templates).unwrap();
        let mut fx = super::input_tests::InputFixture::with_template_inventory(&inventory);
        fx.run
            .context
            .begin_template_checks(inventory, &mut fx.run.clock)
            .unwrap();
        let now = fx.run.clock.now().unwrap();
        let bound = AbsoluteBound::after(now, 5).unwrap();
        let key = if fatal {
            PumpDeadline::Decision
        } else {
            PumpDeadline::Leave
        };
        fx.run.context.arm(key, bound, &mut fx.run.clock).unwrap();
        fx.run.clock.advance_to(bound.deadline_ns()).unwrap();
        let mut original = event(Kind::TurnStarted, None);
        original.turn = Some(TurnRef("blocked-turn".into()));
        let error = fx.run.author(original.clone()).unwrap_err();
        if fatal {
            assert!(matches!(
                error,
                RunFailure::Flow {
                    reason: FlowHaltReason::DecisionBound,
                    ..
                }
            ));
            assert!(fx.run.context.template_blocked_author.is_none());
            assert_eq!(
                format!("{:?}", fx.run.context.refused_author.as_ref().unwrap()),
                format!("{original:?}")
            );
        } else {
            assert_eq!(error, RunFailure::TemplateBlocked);
            assert_eq!(
                format!(
                    "{:?}",
                    fx.run.context.template_blocked_author.as_ref().unwrap()
                ),
                format!("{original:?}")
            );
            assert!(fx.run.context.refused_author.is_none());
            assert_eq!(
                fx.run.context.complete_wait(PumpDeadline::Leave),
                Some(bound)
            );
            assert!(
                fx.run
                    .context
                    .resume_author_and_wait(&mut author_ports(&mut fx.run.clock))
                    .map_err(|failure| failure.retained_reason())
                    .is_err()
            );
        }
        assert!(fx.run.context.waiting_content.is_none());
        assert!(!fx.run.context.author_admission_pending);
        assert!(fx.run.context.pending_author().is_none());
        assert!(fx.records(Kind::TurnStarted).is_empty());
    }
}
