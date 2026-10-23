//! Actual startup sockets and fork custody; child protocol replies are scripted.
use super::*;
#[path = "context_enter_startup_tests.rs"]
pub(in crate::flow::context) mod entered_tests;
use crate::flow::clock::{ReadyFd, WaitInterest, WaitOutcome, testing::FakePumpIo};
use crate::flow::context::edge_tests::classifier_fixture;
use crate::flow::context::failed_enter_driver::ChildState;
use crate::flow::context::template_tests::inventory;
use crate::flow::context::tests::{Fixture, config};
use crate::flow::coordinator::tests::operation;
use crate::flow::ste100_channel::{Channel, Receive, Tag};
use nix::poll::PollFlags;
use std::io::{Read, Write};
use std::os::fd::FromRawFd;
use std::os::unix::fs::PermissionsExt;
use std::time::{Duration, Instant};
use weaver_types::state_flow as s;

pub(super) struct CircuitIo<'a>(pub(super) &'a mut FakePumpIo);
impl PumpIo for CircuitIo<'_> {
    fn now(&mut self) -> std::io::Result<u64> {
        self.0.now()
    }
    fn wait(
        &mut self,
        interests: &[WaitInterest<'_>],
        deadline: Option<u64>,
    ) -> std::io::Result<WaitOutcome> {
        let at = self.0.now()? + 1;
        self.0.schedule(
            at,
            interests
                .iter()
                .enumerate()
                .filter_map(|(index, i)| {
                    let events = i.events & (PollFlags::POLLIN | PollFlags::POLLOUT);
                    (!events.is_empty()).then_some(ReadyFd { index, events })
                })
                .collect(),
        )?;
        self.0.wait(interests, deadline)
    }
}

#[test]
fn startup_composition_child() {
    let Ok(mode) = std::env::var("WEAVER_STARTUP_COMPOSITION_CHILD") else {
        return;
    };
    // SAFETY: the actual fork producer exclusively maps this child's endpoint to 3.
    let fd = unsafe { std::os::fd::OwnedFd::from_raw_fd(3) };
    let mut channel = Channel::from_owned(fd, 262144).unwrap();
    let limit = Instant::now() + Duration::from_secs(10);
    let mut refusing = false;
    while Instant::now() < limit {
        match channel.try_receive() {
            Ok(Receive::Complete(message)) => {
                if message.tag == Tag::Bootstrap {
                    let bootstrap: s::Ste100Bootstrap =
                        serde_json::from_slice(&message.payload).unwrap();
                    let (_, _, mut ready) = classifier_fixture();
                    ready.max_wire_bytes = bootstrap.instruction.max_wire_bytes;
                    if mode == "refuse" {
                        refusing = true;
                        channel
                            .begin_send(
                                Tag::AdmissionRefused,
                                0,
                                serde_json::to_vec(&s::Ste100AdmissionRefused {
                                    protocol: s::STE100_PROTOCOL.into(),
                                    code: s::Ste100AdmissionCode::UnsupportedArtifact,
                                    detail: "scripted startup refusal".into(),
                                })
                                .unwrap(),
                            )
                            .unwrap();
                    } else {
                        channel
                            .begin_send(Tag::Ready, 0, serde_json::to_vec(&ready).unwrap())
                            .unwrap();
                    }
                } else {
                    assert_eq!(message.tag, Tag::Request);
                    let request: s::Ste100Request =
                        serde_json::from_slice(&message.payload).unwrap();
                    let reply = s::Ste100Reply {
                        request_id: request.request_id,
                        target: request.target,
                        scheme_id: request.scheme_id,
                        result: s::Ste100Result::Accepted { bindings: vec![] },
                    };
                    channel
                        .begin_send(Tag::Reply, message.id, serde_json::to_vec(&reply).unwrap())
                        .unwrap();
                }
            }
            Ok(_) => {}
            Err(_) => return,
        }
        if channel.has_pending_send() {
            channel.try_send().unwrap();
        }
        if refusing && !channel.has_pending_send() {
            // SAFETY: this is the isolated exec child, after its complete refusal.
            unsafe { nix::libc::_exit(1) };
        }
        std::thread::sleep(Duration::from_millis(1));
    }
    panic!("startup child watchdog");
}

struct ChildFixture {
    path: PathBuf,
}
impl ChildFixture {
    fn new(mode: &str) -> Self {
        static NEXT: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
        let id = NEXT.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        let path =
            std::env::temp_dir().join(format!("weaver-startup-child-{}-{id}", std::process::id()));
        let exe = std::env::current_exe().unwrap();
        let quote = |s: &str| format!("'{}'", s.replace('\'', "'\\''"));
        std::fs::write(&path, format!("#!/bin/sh\nexport WEAVER_STARTUP_COMPOSITION_CHILD={}\nexec {} --exact flow::context::startup_composition::tests::startup_composition_child --nocapture\n", quote(mode), quote(exe.to_str().unwrap()))).unwrap();
        std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o700)).unwrap();
        Self { path }
    }
}
impl Drop for ChildFixture {
    fn drop(&mut self) {
        let _ = std::fs::remove_file(&self.path);
    }
}

struct ResourceFixture(Option<FailedEnterResources>);
impl std::ops::Deref for ResourceFixture {
    type Target = FailedEnterResources;
    fn deref(&self) -> &Self::Target {
        self.0.as_ref().unwrap()
    }
}
impl std::ops::DerefMut for ResourceFixture {
    fn deref_mut(&mut self) -> &mut Self::Target {
        self.0.as_mut().unwrap()
    }
}
impl Drop for ResourceFixture {
    fn drop(&mut self) {
        if let Some(resources) = &mut self.0 {
            resources.reap_startup_test_children();
        }
    }
}

#[derive(Clone, Copy)]
enum Case {
    Ready,
    Refused,
    RefusalJoin(&'static str),
    Partition,
    Expiry,
    DelayedOpen,
    DelayedGrants,
    RefuseGrants,
    GrantsPressure,
    ExpireGrants,
    ReplaceGrantsEnter,
    ThroughBoot,
    RefuseOpen,
    Residency(&'static str),
    Entered(&'static str),
}

fn circuit(case: Case) {
    let grants_refused = matches!(
        case,
        Case::RefuseGrants
            | Case::RefusalJoin("response-initial-grants-refused" | "response-both-grants-refused")
    );
    let child = ChildFixture::new(
        if matches!(case, Case::Refused)
            || matches!(case, Case::RefusalJoin(mode) if mode != "ready")
        {
            "refuse"
        } else {
            "ready"
        },
    );
    let (bootstrap, scheme, ready) = classifier_fixture();
    let mut declaration = config();
    declaration.scheme = scheme;
    if matches!(case, Case::Partition) {
        declaration.partition[0].kind = "save_point".into();
    }
    let mut run = Fixture::new(declaration);
    let origin = run.context.origin_ns();
    let enter = run.context.deadlines.get(PumpDeadline::Enter).unwrap();
    let inventory = inventory();
    let mut load = run.load_event();
    let Some(Payload::Elections(record)) = &mut load.payload else {
        unreachable!()
    };
    record.ste100_scheme = bootstrap.scheme_text.clone();
    record.state_flow.ste100 = crate::flow::mirror::instruction(&bootstrap.instruction);
    record.template_inventory = inventory.record(
        &run.context.declaration.scheme,
        &bootstrap.instruction.model_artifact_sha256,
    );
    // These values cannot be known before the reply. The composer must use reality.
    record.origin_ns = 999.into();
    record.state_member_observation.pid = 999;
    let start = serde_json::from_value(serde_json::to_value(&record.state_start).unwrap()).unwrap();
    let mut original = None;
    let mut original_peer = None;
    if matches!(case, Case::Entered(_)) {
        let (source, peer) = entered_tests::adopt(&mut run, record, &bootstrap, &child.path);
        original = Some(source);
        original_peer = Some(peer);
    }
    let mut resources = ResourceFixture(Some(run.context.failed_enter_resources().unwrap()));
    let request = StartupRequest {
        start,
        limits: s::ReadLimits {
            max_result_bytes: 100000.into(),
            measurement_ref: "fixture".into(),
        },
        load,
        bootstrap,
        classifier_binary: child.path.clone(),
        inventory,
    };
    let mut owner = if let Some(original) = original {
        let mut inputs = EnterStartupInputs {
            original,
            composer: weaver_trace::LoopIdentity::compiled("worker"),
            classifier_binary: child.path.clone(),
            inventory: request.inventory,
        };
        if let Case::Entered(mode @ ("foreign-context" | "foreign-resources" | "missing-checker")) =
            case
        {
            let mut foreign = Fixture::new(config());
            let mut foreign_resources = foreign.context.failed_enter_resources().unwrap();
            if mode == "missing-checker" {
                inputs.classifier_binary = PathBuf::from("/synthetic/not-in-stack");
            }
            let result = match mode {
                "foreign-context" => StartupComposition::from_enter(
                    &mut foreign.context,
                    &mut foreign_resources,
                    inputs,
                ),
                "foreign-resources" => {
                    StartupComposition::from_enter(&mut run.context, &mut foreign_resources, inputs)
                }
                _ => StartupComposition::from_enter(&mut run.context, &mut resources, inputs),
            };
            let (returned, reason) = result.err().expect("unrelated Enter input accepted");
            assert!(!reason.is_empty());
            assert!(returned.original.adopted_by(&run.context));
            assert_eq!(run.context.phase(), RecordingPhase::Preflight);
            assert!(resources.classifier_child().is_none());
            return;
        }
        match StartupComposition::from_enter(&mut run.context, &mut resources, inputs) {
            Ok(owner) => owner,
            Err((_, reason)) => panic!("Enter join refused: {reason}"),
        }
    } else {
        StartupComposition::new(&mut run.context, &mut resources, request)
    };
    if matches!(case, Case::Entered("eof")) {
        drop(original_peer.take());
    }
    if matches!(case, Case::Entered("stop")) {
        let stop = weaver_types::OrganEnvelope {
            exchange: weaver_types::ExchangeId {
                opener: weaver_types::Opener::Admin,
                ordinal: 8,
            },
            position: weaver_types::Position::Open,
            payload: weaver_types::Payload::Directive(weaver_types::LifecycleDirective::Stop {
                cause: weaver_types::Cause { uid: 73 },
            }),
        };
        use std::os::fd::AsRawFd;
        nix::sys::socket::send(
            original_peer.as_ref().unwrap().as_raw_fd(),
            &serde_json::to_vec(&stop).unwrap(),
            nix::sys::socket::MsgFlags::MSG_NOSIGNAL,
        )
        .unwrap();
    }
    run.state.set_nonblocking(true).unwrap();
    let mut bytes = Vec::new();
    let mut opened = None;
    let mut delayed = 0;
    let mut opens = 0;
    let mut grants = 0;
    let mut grant_answer = None;
    let mut expected_grants: Option<Vec<u8>> = None;
    let mut delayed_grants = 0;
    let mut pressure_steps = 0;
    let mut terminal = false;
    for _ in 0..3000 {
        // SAFETY: this test thread retains and reaps the child before returning.
        let stage =
            unsafe { owner.advance(LifecyclePorts::default(), &mut CircuitIo(&mut run.clock)) };
        assert_eq!(owner.context.origin_ns(), origin);
        if !matches!(case, Case::ReplaceGrantsEnter) || grant_answer.is_none() {
            assert_eq!(
                owner.context.deadlines.get(PumpDeadline::Enter),
                Some(enter)
            );
        }
        if matches!(stage, StartupStage::Stopped | StartupStage::Complete)
            && let Case::RefusalJoin(mode) = case
        {
            super::refusal::tests::drive(
                owner,
                &mut run.clock,
                &mut run._reader,
                &mut run.state,
                mode,
            );
            resources.reap_startup_test_children();
            return;
        }
        if stage == StartupStage::Stopped {
            if let Case::Entered(mode @ ("eof" | "stop")) = case {
                if mode == "eof" {
                    assert!(matches!(
                        owner.stopped(),
                        Some(StartupStop::Run(RunFailure::Transport {
                            source: PumpSource::Admin,
                            ..
                        }))
                    ));
                } else {
                    assert!(matches!(owner.stopped(), Some(StartupStop::Interrupted)));
                }
                let cause = format!("{:?}", owner.stopped());
                drop(original_peer.take());
                // SAFETY: a stopped owner performs no fork, and this thread owns cleanup.
                unsafe { owner.advance(LifecyclePorts::default(), &mut run.clock) };
                assert_eq!(format!("{:?}", owner.stopped()), cause);
                let (_, children, account) = owner.into_stopped().ok().unwrap();
                assert!(account.original_enter.is_some());
                assert!(account.enter_load.is_some());
                assert!(account.load_sequence.is_none());
                assert!(children.classifier_child().is_none());
                return;
            }
            match case {
                Case::ExpireGrants | Case::ReplaceGrantsEnter => {
                    assert!(matches!(owner.stopped(), Some(StartupStop::Run(_))));
                    assert!(owner.resources.classifier_child().is_none());
                    assert!(owner.initial_grants.is_none());
                    assert!(owner.context.inspection.is_some());
                    assert_eq!(grants, 1);
                }
                Case::RefuseOpen => {
                    assert!(
                        matches!(owner.stopped(), Some(StartupStop::OpenRefused(s::StateRefusalCode::OpenFailed, detail)) if detail == "scripted Open refusal")
                    );
                    assert!(owner.load.is_some());
                    assert!(owner.load_sequence.is_none());
                    assert!(owner.resources.classifier_child().is_none());
                    assert_eq!(owner.context.phase(), RecordingPhase::Preflight);
                }
                Case::Refused => {
                    assert!(
                        matches!(owner.stopped(), Some(StartupStop::ClassifierRefused(r)) if r.detail == "scripted startup refusal")
                    );
                    assert!(owner.context.failure().is_none());
                    assert!(owner.context.flow.standing_classifier().is_none());
                    assert!(owner.resources.classifier_child().is_some());
                }
                Case::Partition => {
                    assert!(matches!(
                        owner.stopped(),
                        Some(StartupStop::Activation(ActivationFailure::Partition(_)))
                    ));
                    assert!(owner.context.failure().is_none());
                    assert!(owner.resources.classifier_child().is_none());
                    assert_eq!(owner.load_sequence, Some(Sequence(0)));
                }
                _ => panic!("unexpected startup stop {:?}", owner.stopped()),
            }
            let (context, children, account) = match owner.into_stopped() {
                Ok(stopped) => stopped,
                Err(_) => panic!("stopped owner did not return custody"),
            };
            assert_eq!(context.origin_ns(), origin);
            if matches!(case, Case::RefuseOpen) {
                assert!(account.load_sequence.is_none());
                assert!(account.load.is_some());
            } else {
                assert_eq!(account.load_sequence, Some(Sequence(0)));
                assert!(account.load.is_none());
            }
            if matches!(case, Case::Refused) {
                assert!(account.opened.is_some());
                assert!(matches!(
                    account.initial_grants,
                    Some(InspectionCompletion::Grants { .. })
                ));
                assert!(matches!(account.cause, StartupStop::ClassifierRefused(_)));
                assert!(children.classifier_child().is_some());
                for _ in 0..3000 {
                    if children.poll_startup_test_children() {
                        break;
                    }
                    std::thread::sleep(Duration::from_millis(1));
                }
                assert!(matches!(
                    children.classifier_child(),
                    Some(ChildState::Reaped(nix::sys::wait::WaitStatus::Exited(_, 1)))
                ));
            }
            terminal = true;
            break;
        }
        if stage == StartupStage::Complete {
            if let Some(original) = owner.original_enter.as_ref() {
                entered_tests::assert_record(owner.context, original, &mut run._reader);
                let weaver_types::state_flow::enter::EnterBinding::Serving {
                    state_management,
                    ste100_scheme,
                    ..
                } = &original.envelope().enter().binding
                else {
                    panic!()
                };
                assert_eq!(&owner.bootstrap.instruction, &state_management.ste100);
                assert_eq!(&owner.bootstrap.scheme_text, ste100_scheme);
            }
            assert_eq!(opens, 1);
            assert_eq!(grants, 1);
            let initial = owner
                .initial_grants
                .as_ref()
                .expect("initial Grants retained");
            assert_initial_grants(
                initial,
                expected_grants.as_ref().unwrap(),
                grants_refused,
                enter,
            );
            assert!(owner.context.failure().is_none());
            assert_eq!(owner.load_sequence, Some(Sequence(0)));
            assert_eq!(owner.classifier.as_ref(), Some(&ready));
            assert!(owner.opened.is_some());
            let records = owner.context.barrier.recorder().structure();
            assert_eq!(records.by_kind(Kind::Load).count(), 1);
            assert_eq!(records.by_kind(Kind::StateStoreMeasurement).count(), 2);
            assert!(owner.context.flow.standing_classifier().is_some());
            if matches!(case, Case::Expiry) {
                run.clock.advance_to(enter.deadline_ns()).unwrap();
                owner = match owner.finish(LifecyclePorts::default(), &mut run.clock) {
                    Err(owner) => owner,
                    Ok(_) => panic!("expired startup granted prepared context"),
                };
                assert!(matches!(
                    owner.stopped(),
                    Some(StartupStop::Run(RunFailure::Wait { .. }))
                ));
                assert!(owner.opened.is_some());
                let (_, _, account) = match owner.into_stopped() {
                    Ok(stopped) => stopped,
                    Err(_) => panic!("expired owner did not return custody"),
                };
                assert!(account.opened.is_some());
                assert_eq!(account.classifier, Some(ready));
            } else {
                let prepared = match owner.finish(LifecyclePorts::default(), &mut run.clock) {
                    Ok(prepared) => prepared,
                    Err(owner) => panic!("startup finish refused {:?}", owner.stopped()),
                };
                if matches!(case, Case::Entered("wrong-spu")) {
                    let mut instruction = prepared
                        .original_enter()
                        .unwrap()
                        .envelope()
                        .enter()
                        .spu_instruction
                        .clone();
                    instruction
                        .decoder
                        .model_binding
                        .artifact
                        .0
                        .push_str("-substitute");
                    let residency = super::super::residency_composition::ResidencyComposition::new(
                        prepared,
                        super::super::residency_composition::ResidencyRequest {
                            binary: PathBuf::from("/not-executed"),
                            arguments: vec![],
                            instruction,
                        },
                    );
                    assert_eq!(
                        residency.stage(),
                        super::super::residency_composition::ResidencyStage::Stopped
                    );
                    let (_, children, account) = residency.into_stopped().ok().unwrap();
                    assert!(children.spu_child().is_none());
                    assert!(account.startup.original_enter.is_some());
                    return;
                }
                if let Case::Residency(mode) | Case::Entered(mode @ "full") = case {
                    let mut stopped = None;
                    if let Some(admitted) = super::super::residency_composition::tests::drive(
                        prepared,
                        &mut run.clock,
                        mode,
                        &run.state,
                        &mut stopped,
                    ) {
                        let (context, admitted_resources, inventory, evidence) =
                            admitted.into_parts();
                        assert_eq!(context.origin_ns(), origin);
                        assert!(
                            admitted_resources
                                .residency_request()
                                .unwrap()
                                .send()
                                .sent()
                        );
                        assert_initial_grants(
                            &evidence.startup.initial_grants,
                            expected_grants.as_ref().unwrap(),
                            false,
                            enter,
                        );
                        if mode == "full" {
                            super::super::boot_composition::tests::after_residency(
                                run,
                                inventory,
                                &mut resources,
                                evidence,
                            );
                            resources.reap_startup_test_children();
                            return;
                        }
                    }
                    if let Some(account) = stopped {
                        super::super::residency_composition::cleanup::tests::drive(
                            &mut run,
                            resources.0.take().unwrap(),
                            account,
                            mode,
                        );
                    } else {
                        resources.reap_startup_test_children();
                    }
                    return;
                }
                let (context, _, inventory, evidence) = prepared.into_parts();
                assert_initial_grants(
                    &evidence.initial_grants,
                    expected_grants.as_ref().unwrap(),
                    grants_refused,
                    enter,
                );
                assert_eq!(evidence.load, Sequence(0));
                assert_eq!(evidence.classifier, ready);
                assert_eq!(context.origin_ns(), origin);
                if matches!(case, Case::ThroughBoot) {
                    super::super::boot_composition::tests::after_startup(run, inventory);
                    resources.reap_startup_test_children();
                    return;
                }
                terminal = true;
                break;
            }
            terminal = true;
            break;
        }
        if matches!(case, Case::GrantsPressure)
            && grants == 1
            && grant_answer.is_none()
            && pressure_steps < 10
        {
            pressure_steps += 1;
            assert_eq!(owner.stage(), StartupStage::InitialGrants);
            assert!(owner.initial_grants.is_none());
            assert!(owner.resources.classifier_child().is_none());
            if pressure_steps == 10 {
                super::refusal::tests::drain(owner.context, &mut run._reader);
            }
        }
        let mut part = [0u8; 65536];
        match run.state.read(&mut part) {
            Ok(n) => bytes.extend_from_slice(&part[..n]),
            Err(e) if e.kind() == std::io::ErrorKind::WouldBlock => {}
            Err(e) => panic!("state peer: {e}"),
        }
        if let Some(end) = bytes.iter().position(|b| *b == b'\n') {
            let line: Vec<_> = bytes.drain(..=end).collect();
            let request = s::decode_state_line(&line, s::FrameDirection::Request).unwrap();
            if let s::StateFrame::Control {
                frame: s::StateControl::Grants {},
                ..
            } = &request
            {
                grants += 1;
                assert_eq!(grants, 1);
                assert!(owner.resources.classifier_child().is_none());
                assert!(owner.opened.is_some());
                assert_eq!(owner.load_sequence, Some(Sequence(0)));
                let now = run.clock.now().unwrap();
                let answer = s::StateFrame::ControlAnswer {
                    common: request.common().clone(),
                    frame: if grants_refused {
                        s::StateControlAnswer::ControlRefused {
                            code: s::ControlRefusalCode::Read(s::ReadRefusalCode::AnswerTooLarge),
                            detail: "exact initial Grants refusal".into(),
                        }
                    } else {
                        s::StateControlAnswer::Grants {
                            surface: vec!["owner:actual".into(), "mode:0750".into()],
                        }
                    },
                    observation: operation(&request, 2, now),
                };
                let encoded = s::encode_state_line(&answer, s::FrameDirection::Response).unwrap();
                expected_grants = Some(encoded.clone());
                grant_answer = Some(encoded);
                if matches!(case, Case::GrantsPressure) {
                    super::refusal::tests::fill(owner.context, &mut run.clock);
                }
                if matches!(case, Case::ExpireGrants) {
                    run.clock.advance_to(enter.deadline_ns()).unwrap();
                }
                if matches!(case, Case::ReplaceGrantsEnter) {
                    owner.context.deadlines.disarm(PumpDeadline::Enter);
                    owner
                        .context
                        .deadlines
                        .arm(
                            PumpDeadline::Enter,
                            AbsoluteBound::from_absolute(enter.began_ns(), enter.deadline_ns() + 1)
                                .unwrap(),
                        )
                        .unwrap();
                }
                continue;
            }
            let observation = operation(&request, 1, run.clock.now().unwrap());
            let s::StateFrame::Open {
                common,
                schema_id,
                scheme_id,
                start,
                read_limits,
            } = request
            else {
                panic!("unexpected startup state traffic")
            };
            opens += 1;
            assert_eq!(opens, 1);
            assert!(owner.resources.classifier_child().is_none());
            assert!(owner.load_sequence.is_none());
            opened = Some(if matches!(case, Case::RefuseOpen) {
                s::StateFrame::Refused {
                    common,
                    code: s::StateRefusalCode::OpenFailed,
                    detail: "scripted Open refusal".into(),
                    block_id: None,
                    observation,
                }
            } else {
                s::StateFrame::Opened {
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
                }
            });
        }
        if opened.is_some() {
            if matches!(case, Case::DelayedOpen) && delayed < 10 {
                delayed += 1;
                assert_eq!(owner.stage(), StartupStage::Open);
                assert!(owner.load_sequence.is_none());
                assert!(owner.resources.classifier_child().is_none());
            } else {
                run.state
                    .write_all(
                        &s::encode_state_line(&opened.take().unwrap(), s::FrameDirection::Response)
                            .unwrap(),
                    )
                    .unwrap();
            }
        }
        if grant_answer.is_some() && !matches!(case, Case::ExpireGrants | Case::ReplaceGrantsEnter)
        {
            if matches!(case, Case::DelayedGrants) && delayed_grants < 10 {
                delayed_grants += 1;
                assert_eq!(owner.stage(), StartupStage::InitialGrants);
                assert!(owner.initial_grants.is_none());
                assert!(owner.resources.classifier_child().is_none());
                assert_eq!(
                    owner
                        .context
                        .deadlines
                        .get(PumpDeadline::StateControl)
                        .unwrap()
                        .deadline_ns(),
                    enter.deadline_ns()
                );
            } else {
                run.state.write_all(&grant_answer.take().unwrap()).unwrap();
            }
        }
        std::thread::sleep(Duration::from_millis(1));
    }
    // Actual child cleanup for this instrument, not a claim about lifecycle Left.
    resources.reap_startup_test_children();
    assert!(terminal, "startup did not finish within fixture watchdog");
}

#[test]
fn startup_composition_records_load_before_actual_classifier_start() {
    circuit(Case::Ready);
}
#[test]
fn startup_composition_preserves_real_classifier_refusal() {
    circuit(Case::Refused);
}
#[test]
fn startup_composition_partition_refusal_never_forks_classifier() {
    circuit(Case::Partition);
}
#[test]
fn startup_composition_expiry_retains_prepared_evidence() {
    circuit(Case::Expiry);
}
#[test]
fn startup_composition_waits_for_actual_structural_open() {
    circuit(Case::DelayedOpen);
}

#[test]
fn startup_composition_drives_into_recorded_boot_and_placement() {
    circuit(Case::ThroughBoot);
}

#[test]
fn startup_composition_open_refusal_retains_unwritten_load_and_never_forks() {
    circuit(Case::RefuseOpen);
}

pub(in crate::flow::context) fn residency(mode: &'static str) {
    circuit(Case::Residency(mode));
}

pub(super) fn refusal_join(mode: &'static str) {
    circuit(Case::RefusalJoin(mode));
}

fn assert_initial_grants(
    completion: &InspectionCompletion,
    expected: &[u8],
    refused: bool,
    enter: AbsoluteBound,
) {
    let evidence = match completion {
        InspectionCompletion::Grants { surface, evidence } if !refused => {
            assert_eq!(
                surface,
                &["owner:actual".to_owned(), "mode:0750".to_owned()]
            );
            evidence
        }
        InspectionCompletion::Refused { failure, evidence } if refused => {
            assert!(
                matches!(failure, crate::flow::read::ReadFailure::Refused { code: s::ReadRefusalCode::AnswerTooLarge, detail } if detail == "exact initial Grants refusal")
            );
            evidence
        }
        _ => panic!("wrong retained initial Grants: {completion:?}"),
    };
    assert_eq!(
        s::encode_state_line(evidence.answer(), s::FrameDirection::Response).unwrap(),
        expected
    );
    assert!(matches!(evidence.request(), s::StateControl::Grants {}));
    assert!(evidence.turn().is_none());
    assert!(!evidence.bound().expired(evidence.recorded_ns()));
    assert!(evidence.bound().deadline_ns() <= enter.deadline_ns());
}

#[test]
fn startup_grants_composition_preserves_initial_measurement_through_residency() {
    circuit(Case::Residency("full"));
}
#[test]
fn startup_grants_composition_refusal_remains_unavailable_without_halting() {
    circuit(Case::RefuseGrants);
}
#[test]
fn startup_grants_composition_waits_before_classifier_fork() {
    circuit(Case::DelayedGrants);
}
#[test]
fn startup_grants_composition_expiry_retains_pending_inspection_without_fork() {
    circuit(Case::ExpireGrants);
}
#[test]
fn startup_grants_composition_replaced_enter_cannot_extend_inspection() {
    circuit(Case::ReplaceGrantsEnter);
}

#[test]
fn startup_grants_composition_trace_pressure_holds_classifier_until_recorded() {
    circuit(Case::GrantsPressure);
}

#[test]
fn enter_startup_composition_records_actual_received_configuration() {
    circuit(Case::Entered("ready"));
}
#[test]
fn enter_startup_composition_preserves_original_peer_eof_and_first_cause() {
    circuit(Case::Entered("eof"));
}
#[test]
fn enter_startup_composition_holds_stop_from_the_original_peer() {
    circuit(Case::Entered("stop"));
}
#[test]
fn enter_startup_composition_keeps_the_original_through_residency_and_boot() {
    circuit(Case::Entered("full"));
}
pub(in crate::flow::context) fn entered_full() {
    circuit(Case::Entered("full"));
}

#[test]
fn enter_startup_composition_refuses_a_foreign_recording_context() {
    circuit(Case::Entered("foreign-context"));
}
#[test]
fn enter_startup_composition_refuses_a_foreign_resource_owner() {
    circuit(Case::Entered("foreign-resources"));
}
#[test]
fn enter_startup_composition_requires_root_checker_digest() {
    circuit(Case::Entered("missing-checker"));
}
#[test]
fn enter_startup_composition_refuses_substituted_spu_instruction_before_fork() {
    circuit(Case::Entered("wrong-spu"));
}
