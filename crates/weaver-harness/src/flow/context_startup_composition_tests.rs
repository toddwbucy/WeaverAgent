//! Actual startup sockets and fork custody; child protocol replies are scripted.
use super::*;
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
                .map(|(index, i)| ReadyFd {
                    index,
                    events: i.events & (PollFlags::POLLIN | PollFlags::POLLOUT),
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

struct ResourceFixture(FailedEnterResources);
impl std::ops::Deref for ResourceFixture {
    type Target = FailedEnterResources;
    fn deref(&self) -> &Self::Target {
        &self.0
    }
}
impl std::ops::DerefMut for ResourceFixture {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.0
    }
}
impl Drop for ResourceFixture {
    fn drop(&mut self) {
        self.0.reap_startup_test_children();
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
    ThroughBoot,
    RefuseOpen,
    Residency(&'static str),
}

fn circuit(case: Case) {
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
    let mut resources = ResourceFixture(run.context.failed_enter_resources().unwrap());
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
    let mut owner = StartupComposition::new(&mut run.context, &mut resources, request);
    run.state.set_nonblocking(true).unwrap();
    let mut bytes = Vec::new();
    let mut opened = None;
    let mut delayed = 0;
    let mut opens = 0;
    let mut terminal = false;
    for _ in 0..3000 {
        // SAFETY: this test thread retains and reaps the child before returning.
        let stage =
            unsafe { owner.advance(LifecyclePorts::default(), &mut CircuitIo(&mut run.clock)) };
        assert_eq!(owner.context.origin_ns(), origin);
        assert_eq!(
            owner.context.deadlines.get(PumpDeadline::Enter),
            Some(enter)
        );
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
            match case {
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
            assert_eq!(opens, 1);
            assert_eq!(owner.load_sequence, Some(Sequence(0)));
            assert_eq!(owner.classifier.as_ref(), Some(&ready));
            assert!(owner.opened.is_some());
            let records = owner.context.barrier.recorder().structure();
            assert_eq!(records.by_kind(Kind::Load).count(), 1);
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
                if let Case::Residency(mode) = case {
                    if let Some(admitted) = super::super::residency_composition::tests::drive(
                        prepared,
                        &mut run.clock,
                        mode,
                    ) {
                        let (context, _, inventory, evidence) = admitted.into_parts();
                        assert_eq!(context.origin_ns(), origin);
                        assert!(evidence.send.sent());
                        if mode == "full" {
                            super::super::boot_composition::tests::after_residency(
                                run,
                                inventory,
                                &mut resources,
                            );
                            resources.reap_startup_test_children();
                            return;
                        }
                    }
                    resources.reap_startup_test_children();
                    return;
                }
                let (context, _, inventory, evidence) = prepared.into_parts();
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
        let mut part = [0u8; 65536];
        match run.state.read(&mut part) {
            Ok(n) => bytes.extend_from_slice(&part[..n]),
            Err(e) if e.kind() == std::io::ErrorKind::WouldBlock => {}
            Err(e) => panic!("state peer: {e}"),
        }
        if let Some(end) = bytes.iter().position(|b| *b == b'\n') {
            let line: Vec<_> = bytes.drain(..=end).collect();
            let request = s::decode_state_line(&line, s::FrameDirection::Request).unwrap();
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
