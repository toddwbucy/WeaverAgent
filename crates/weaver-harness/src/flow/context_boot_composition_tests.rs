//! One caller drives every boot stage. The socket peers are explicit contract
//! doubles; these tests do not qualify a classifier, model or live SQL member.
use super::super::template_tests::{TemplateFixture, inventory};
use super::*;
use crate::channel::DecodeChannel;
use crate::flow::clock::{ReadyFd, WaitInterest, WaitOutcome, testing::FakePumpIo};
use crate::flow::coordinator::tests::operation;
use crate::flow::ste100_channel::{Receive, Tag};
use nix::errno::Errno;
use nix::poll::PollFlags;
use nix::sys::socket::{MsgFlags, recv, send};
use std::io::{self, Read, Write};
use std::os::fd::AsRawFd;
use weaver_types::state_flow as s;
use weaver_types::{Opened, SessionId, TokenAnswer, TokenDirective};

struct CircuitIo<'a>(&'a mut FakePumpIo);
impl PumpIo for CircuitIo<'_> {
    fn now(&mut self) -> io::Result<u64> {
        self.0.now()
    }
    fn wait(
        &mut self,
        interests: &[WaitInterest<'_>],
        deadline: Option<u64>,
    ) -> io::Result<WaitOutcome> {
        let at = self.0.now()? + 1;
        let ready = interests
            .iter()
            .enumerate()
            .filter_map(|(index, interest)| {
                let events = interest.events & (PollFlags::POLLIN | PollFlags::POLLOUT);
                (!events.is_empty()).then_some(ReadyFd { index, events })
            })
            .collect();
        self.0.schedule(at, ready)?;
        self.0.wait(interests, deadline)
    }
}

pub(in crate::flow::context) fn prompt(seed: &str) -> Vec<u8> {
    let corpus: Vec<serde_json::Value> =
        serde_json::from_str(include_str!("fixtures/recall-sources.json")).unwrap();
    let mut result = corpus
        .iter()
        .find(|v| v["case"] == "system_prompt_nonboot")
        .unwrap()["result"]
        .clone();
    let entry = &mut result["entries"]["entries"][0];
    entry["source"]["seed"]["seed_id"] = seed.into();
    entry["selected_span"]["span_ref"]["seed_id"] = seed.into();
    // Match this run's actual seeded prompt allowance, including UTF-8 byte ends.
    entry["text"] = "Read caf\u{e9}.".into();
    entry["raw_block"]["text"] = "Read caf\u{e9}.".into();
    entry["selected_span"]["end"] = "11".into();
    entry["bindings"][0]["value"]["statement_span"]["end"] = "11".into();
    entry["bindings"][0]["value"]["nouns"][0]["value"] = "caf\u{e9}".into();
    entry["bindings"][0]["value"]["nouns"][0]["span"]["end"] = "10".into();
    serde_json::to_vec(&result).unwrap()
}

#[derive(Clone, Copy)]
enum Fault {
    None,
    RepairPairs,
    HoldPromptRelease,
    RejectTemplate,
    RefusePrompt,
    ZeroGeneration,
    StopBeforeInstall,
    ExpireBeforeFinish,
}

fn circuit(fault: Fault) {
    circuit_with_fixture(fault, TemplateFixture::new(), inventory(), false, None);
}

pub(in crate::flow::context) fn after_startup(
    run: super::super::tests::Fixture,
    inventory: crate::flow::templates::CompiledInventory,
) {
    let (end, _unused) = crate::channel::ClassifyChannel::pair().unwrap();
    let peer =
        crate::flow::ste100_channel::Channel::from_owned(end.into_pump_fd().unwrap(), 262144)
            .unwrap();
    let fx = TemplateFixture {
        input: super::super::input_tests::InputFixture { run, peer },
    };
    circuit_with_fixture(Fault::None, fx, inventory, true, None);
}

pub(in crate::flow::context) fn after_residency(
    mut run: super::super::tests::Fixture,
    inventory: crate::flow::templates::CompiledInventory,
    resources: &mut FailedEnterResources,
    mut evidence: ResidencyEvidence,
) {
    let mode = std::env::var("WEAVER_GATE_STARTUP_DRIVER").unwrap_or_default();
    if mode == "resident-foreign-context" {
        let mut foreign = super::super::tests::Fixture::new(super::super::tests::config());
        evidence = match BootComposition::from_residency(
            &mut foreign.context,
            resources,
            inventory.clone(),
            evidence,
        ) {
            Err((_, _, _, evidence)) => evidence,
            Ok(_) => panic!("foreign context accepted actual residency"),
        };
        assert_eq!(foreign.context.phase, RecordingPhase::Preflight);
        assert!(foreign.context.failure().is_none());
    }
    if mode == "resident-wrong-instruction" {
        let original = evidence.request.instruction.clone();
        evidence
            .request
            .instruction
            .decoder
            .model_binding
            .artifact
            .0
            .push_str("-changed");
        evidence = match BootComposition::from_residency(
            &mut run.context,
            resources,
            inventory.clone(),
            evidence,
        ) {
            Err((_, _, _, evidence)) => evidence,
            Ok(_) => panic!("changed Admit instruction accepted"),
        };
        evidence.request.instruction = original;
        assert!(run.context.failure().is_none());
    }
    if mode == "resident-foreign-bound" {
        let bound = evidence.enter;
        evidence.enter =
            AbsoluteBound::after(bound.began_ns(), bound.deadline_ns() - bound.began_ns() + 1)
                .unwrap();
        evidence = match BootComposition::from_residency(
            &mut run.context,
            resources,
            inventory.clone(),
            evidence,
        ) {
            Err((_, _, _, evidence)) => evidence,
            Ok(_) => panic!("foreign Enter bound accepted"),
        };
        evidence.enter = bound;
        assert!(run.context.failure().is_none());
    }
    let (end, _unused) = crate::channel::ClassifyChannel::pair().unwrap();
    let peer =
        crate::flow::ste100_channel::Channel::from_owned(end.into_pump_fd().unwrap(), 262144)
            .unwrap();
    let fx = TemplateFixture {
        input: super::super::input_tests::InputFixture { run, peer },
    };
    circuit_with_fixture(
        Fault::None,
        fx,
        inventory,
        true,
        Some((resources, evidence)),
    );
}

fn circuit_with_fixture(
    fault: Fault,
    mut fx: TemplateFixture,
    inventory: crate::flow::templates::CompiledInventory,
    actual_child: bool,
    actual_resident: Option<(&mut FailedEnterResources, ResidencyEvidence)>,
) {
    let enter = fx
        .input
        .run
        .context
        .deadlines
        .get(PumpDeadline::Enter)
        .unwrap();
    let origin = fx.input.run.context.origin_ns();
    let seed = fx
        .input
        .run
        .context
        .open_start
        .as_ref()
        .unwrap()
        .seed_id
        .clone();
    let (channel, decoder) = DecodeChannel::pair().unwrap();
    let mut fallback_decode = DecodePort::from_channel(channel).unwrap();
    let actual_spu = actual_resident.is_some();
    fx.input.run.state.set_nonblocking(true).unwrap();
    let mut boot = if let Some((resources, evidence)) = actual_resident {
        match BootComposition::from_residency(
            &mut fx.input.run.context,
            resources,
            inventory,
            evidence,
        ) {
            Ok(boot) => boot,
            Err(_) => panic!("actual residency failed its boot join"),
        }
    } else {
        BootComposition::new(&mut fx.input.run.context, &mut fallback_decode, inventory)
    };
    let resident_mode = std::env::var("WEAVER_GATE_STARTUP_DRIVER").unwrap_or_default();
    if resident_mode == "entered-eof-boot" || resident_mode == "entered-substitute-boot" {
        let (near, _far) = crate::channel::OrganChannel::pair().unwrap();
        let mut substitute = crate::flow::pump::LifecyclePeer::from_channel(near).unwrap();
        if resident_mode == "entered-eof-boot" {
            super::super::startup_composition::tests::entered_tests::close_original_peer();
        }
        let result = boot.advance(
            LifecyclePorts {
                peer: (resident_mode == "entered-substitute-boot").then_some(&mut substitute),
                ..Default::default()
            },
            &mut fx.input.run.clock,
        );
        assert_eq!(result, BootCompositionProgress::Stopped);
        assert!(matches!(
            boot.stopped(),
            Some(BootCompositionStop::Run(RunFailure::Transport {
                source: PumpSource::Admin,
                ..
            }))
        ));
        let stopped = boot.into_stopped().ok().unwrap();
        assert!(
            stopped
                .resident
                .as_ref()
                .unwrap()
                .evidence
                .startup
                .original_enter
                .is_some()
        );
        return;
    }
    if resident_mode == "resident-spu-eof-boot" {
        boot.resident.as_mut().unwrap().spu.stop_at_cap();
        for _ in 0..3000 {
            if boot.advance(
                LifecyclePorts::default(),
                &mut CircuitIo(&mut fx.input.run.clock),
            ) == BootCompositionProgress::Stopped
            {
                break;
            }
            std::thread::sleep(std::time::Duration::from_millis(1));
        }
        assert!(matches!(
            boot.stopped(),
            Some(BootCompositionStop::Run(RunFailure::Transport {
                source: PumpSource::Residency,
                ..
            }))
        ));
        let stopped = match boot.into_stopped() {
            Ok(x) => x,
            Err(_) => panic!("SPU EOF did not stop boot"),
        };
        assert!(stopped.resident.is_some());
        return;
    }
    if matches!(
        resident_mode.as_str(),
        "resident-boot-substitute" | "resident-boot-window" | "resident-stopped-boot"
    ) {
        if resident_mode == "resident-boot-window" {
            boot.context.deadlines.disarm(PumpDeadline::Enter);
            boot.context
                .deadlines
                .arm(
                    PumpDeadline::Enter,
                    AbsoluteBound::after(
                        enter.began_ns(),
                        enter.deadline_ns() - enter.began_ns() + 1,
                    )
                    .unwrap(),
                )
                .unwrap();
        }
        if resident_mode == "resident-stopped-boot" {
            boot.context.fail(
                &mut fx.input.run.clock,
                FlowHaltReason::ProtocolFailed,
                HaltPoint::Barrier,
                "stop actual resident boot",
            );
        }
        let (near, _far) = crate::channel::OrganChannel::pair().unwrap();
        let mut substitute = crate::flow::pump_transport::OrganPort::from_channel(near).unwrap();
        let result = boot.advance(
            LifecyclePorts {
                residency: (resident_mode == "resident-boot-substitute").then_some(&mut substitute),
                ..Default::default()
            },
            &mut fx.input.run.clock,
        );
        assert_eq!(result, BootCompositionProgress::Stopped);
        let stopped = match boot.into_stopped() {
            Ok(x) => x,
            Err(_) => panic!("stopped boot lost custody"),
        };
        let arms = stopped
            .resident
            .as_ref()
            .expect("stopped boot retains actual resident arms");
        assert_eq!(arms.evidence.enter, enter);
        assert!(arms.request.send().sent());
        assert!(arms.evidence.reply.belongs_to(arms.residency));
        assert_eq!(
            arms.spu.state(),
            super::super::failed_enter_driver::ChildState::Running
        );
        assert_eq!(
            arms.classifier.state(),
            super::super::failed_enter_driver::ChildState::Running
        );
        assert!(stopped.context.failure().is_some());
        return;
    }
    let mut input = Vec::new();
    let mut queries = Vec::new();
    let mut releases = 0;
    let mut checks = 0;
    let mut opens = 0;
    let mut filed_pairs = 0;
    let mut operation_id = if actual_child { 3 } else { 2 };
    let mut snapshot = 10u64;
    let mut held_release: Option<(s::StateFrame, usize)> = None;
    let mut held_steps = 0;
    for _ in 0..4096 {
        if actual_child {
            std::thread::sleep(std::time::Duration::from_millis(1));
        }
        if let Some((response, remaining)) = held_release.as_mut() {
            assert_eq!(boot.stage(), BootStage::Prompt);
            assert!(boot.context.boot_head.is_none());
            assert_eq!(opens, 0);
            held_steps += 1;
            *remaining -= 1;
            if *remaining == 0 {
                fx.input
                    .run
                    .state
                    .write_all(
                        &s::encode_state_line(response, s::FrameDirection::Response).unwrap(),
                    )
                    .unwrap();
                held_release = None;
            }
        }

        if boot.stage() == BootStage::Install && matches!(fault, Fault::StopBeforeInstall) {
            // Actual failure on the same context; no forged owner state.
            boot.context.fail(
                &mut fx.input.run.clock,
                FlowHaltReason::ProtocolFailed,
                HaltPoint::Barrier,
                "composition fixture failure before install",
            );
        }
        let progress = boot.advance(
            LifecyclePorts {
                residency: None,
                ..LifecyclePorts::default()
            },
            &mut CircuitIo(&mut fx.input.run.clock),
        );
        assert_eq!(boot.context.origin_ns(), origin);
        assert_eq!(boot.context.deadlines.get(PumpDeadline::Enter), Some(enter));
        if progress == BootCompositionProgress::Stopped {
            match fault {
                Fault::RejectTemplate => {
                    assert!(matches!(
                        boot.stopped(),
                        Some(BootCompositionStop::TemplateRejected)
                    ));
                    assert!(queries.is_empty());
                    assert_eq!(opens, 0);
                }
                Fault::RefusePrompt => {
                    assert!(matches!(
                        boot.stopped(),
                        Some(BootCompositionStop::Prompt(BootReadCompletion::Refused(_)))
                    ));
                    assert_eq!(queries, ["prompt"]);
                    assert_eq!(opens, 0);
                }
                Fault::StopBeforeInstall => {
                    assert!(matches!(boot.stopped(), Some(BootCompositionStop::Run(_))));
                    assert!(boot.summary.is_some());
                    assert!(boot.context.place_engine.is_none());
                }
                Fault::ZeroGeneration => {
                    assert!(matches!(
                        boot.stopped(),
                        Some(BootCompositionStop::Open(BootOpenProgress::Refused))
                    ));
                    assert_eq!(queries, ["prompt"]);
                    assert_eq!(releases, 1);
                    assert_eq!(opens, 1);
                    assert!(boot.context.failure().is_none());
                    assert!(boot.context.boot_journal.is_none());
                    assert!(boot.context.place_engine.is_none());
                    assert!(boot.prior.is_none());
                    assert!(boot.repaired.is_none());
                    assert!(boot.summary.is_none());
                    assert!(matches!(
                        boot.context.boot_open_refusal(),
                        Some(weaver_types::LifecycleRefusal::StatePersistenceInvalid {
                            stage: s::PersistenceStage::Boot,
                            case: s::PersistenceCase::BootFailed,
                            ..
                        })
                    ));
                }
                _ => panic!(
                    "unexpected stop at {:?}: {:?}",
                    boot.stage(),
                    boot.stopped()
                ),
            }
            let stopped = match boot.finish(
                LifecyclePorts {
                    residency: None,
                    ..LifecyclePorts::default()
                },
                &mut fx.input.run.clock,
            ) {
                Err(owner) => match owner.into_stopped() {
                    Ok(stopped) => stopped,
                    Err(_) => panic!("stopped owner not returned"),
                },
                Ok(_) => panic!("stopped boot granted completion"),
            };
            assert_eq!(stopped.context.origin_ns(), origin);
            if matches!(fault, Fault::StopBeforeInstall) {
                assert!(stopped.summary.is_some());
                assert!(matches!(stopped.cause, BootCompositionStop::Run(_)));
            }
            return;
        }
        if progress == BootCompositionProgress::Complete {
            assert_eq!(
                filed_pairs,
                if matches!(fault, Fault::RepairPairs) {
                    2
                } else {
                    0
                }
            );
            if matches!(fault, Fault::HoldPromptRelease) {
                assert_eq!(held_steps, 10);
            }
            assert_eq!(checks, if actual_child { 0 } else { 3 });
            assert_eq!(opens, if actual_spu { 0 } else { 1 });
            assert_eq!(releases, 4);
            assert_eq!(queries, ["prompt", "pairs", "pairs", "summary"]);
            assert!(boot.context.place_engine.is_some());
            assert!(boot.context.committed_plan_summary().is_ok());
            assert!(boot.context.boot_head.is_some());
            if matches!(fault, Fault::ExpireBeforeFinish) {
                fx.input.run.clock.advance_to(enter.deadline_ns()).unwrap();
                let stopped = match boot.finish(
                    LifecyclePorts {
                        residency: None,
                        ..LifecyclePorts::default()
                    },
                    &mut fx.input.run.clock,
                ) {
                    Err(owner) => owner,
                    Ok(_) => panic!("expired boot transferred completion"),
                };
                assert!(matches!(
                    stopped.stopped(),
                    Some(BootCompositionStop::Run(RunFailure::Wait { .. }))
                ));
                assert!(stopped.context.place_engine.is_some());
            } else {
                let completed = match boot.finish(
                    LifecyclePorts {
                        residency: None,
                        ..LifecyclePorts::default()
                    },
                    &mut fx.input.run.clock,
                ) {
                    Ok(completed) => completed,
                    Err(owner) => panic!("completed sequence refused: {:?}", owner.stopped()),
                };
                if actual_spu {
                    let arms = completed
                        .resident
                        .as_ref()
                        .expect("completed boot retained resident arms");
                    assert_eq!(arms.evidence.enter, enter);
                    assert!(arms.evidence.reply.belongs_to(arms.residency));
                    assert!(arms.request.send().sent());
                    assert_eq!(arms.evidence.startup.load, Sequence(0));
                }
                assert_eq!(completed.certificate.accepted().len(), 3);
                assert_eq!(completed.context.origin_ns(), origin);
                assert!(completed.decode.held().is_none());
                assert!(completed.context.input.snapshot().is_none());
                if let Ok(mode) = std::env::var("WEAVER_GATE_STARTUP_DRIVER") {
                    assert!(
                        actual_child && actual_spu,
                        "gate instrument uses actual prior forks"
                    );
                    super::gate_startup::tests::drive(
                        completed,
                        &mut fx.input.run.clock,
                        None,
                        &mode,
                    );
                }
            }
            return;
        }
        if !actual_child && let Receive::Complete(message) = fx.input.peer.try_receive().unwrap() {
            assert_eq!(message.tag, Tag::Request);
            let request: s::Ste100Request = serde_json::from_slice(&message.payload).unwrap();
            checks += 1;
            let result = if matches!(fault, Fault::RejectTemplate) {
                s::Ste100Result::NotSte100 {
                    code: s::Ste100DefectCode::NonSte100,
                    detail: "fixture template rejection".into(),
                    spans: vec![],
                }
            } else {
                s::Ste100Result::Accepted { bindings: vec![] }
            };
            let answer = s::Ste100Reply {
                request_id: request.request_id,
                target: request.target,
                scheme_id: request.scheme_id,
                result,
            };
            fx.input
                .peer
                .begin_send(Tag::Reply, message.id, serde_json::to_vec(&answer).unwrap())
                .unwrap();
        }
        if !actual_child && fx.input.peer.has_pending_send() {
            fx.input.peer.try_send().unwrap();
        }
        let mut bytes = [0; 65536];
        match fx.input.run.state.read(&mut bytes) {
            Ok(0) => panic!("member unexpectedly closed"),
            Ok(n) => input.extend_from_slice(&bytes[..n]),
            Err(e) if e.kind() == io::ErrorKind::WouldBlock => {}
            Err(e) => panic!("member read: {e}"),
        }
        while let Some(end) = input.iter().position(|b| *b == b'\n') {
            let line: Vec<_> = input.drain(..=end).collect();
            let request = s::decode_state_line(&line, s::FrameDirection::Request).unwrap();
            if let s::StateFrame::FilePair { filing, .. } = &request {
                assert!(matches!(fault, Fault::RepairPairs));
                assert_eq!(queries, ["prompt", "pairs"]);
                assert_eq!(opens, if actual_spu { 0 } else { 1 });
                let now = fx.input.run.clock.now().unwrap();
                let response = s::StateFrame::Filed {
                    common: request.common().clone(),
                    block_id: filing.block_id.clone(),
                    observation: operation(&request, operation_id, now),
                };
                operation_id += 1;
                filed_pairs += 1;
                fx.input
                    .run
                    .state
                    .write_all(
                        &s::encode_state_line(&response, s::FrameDirection::Response).unwrap(),
                    )
                    .unwrap();
                continue;
            }
            let s::StateFrame::Control { frame, .. } = &request else {
                panic!("unexpected state request {request:?}")
            };
            let answer = match frame {
                s::StateControl::Read { selector } => {
                    let raw = match selector {
                        s::ReadSelector::SystemPrompt {} => {
                            queries.push("prompt");
                            prompt(&seed)
                        }
                        s::ReadSelector::OpenPairs {} => {
                            queries.push("pairs");
                            if matches!(fault, Fault::RepairPairs) && queries.len() == 2 {
                                let corpus: Vec<serde_json::Value> = serde_json::from_str(
                                    include_str!("fixtures/recall-sources.json"),
                                )
                                .unwrap();
                                let pair = corpus
                                    .iter()
                                    .find(|v| v["case"] == "open_pairs_original_blocks")
                                    .unwrap()["result"]["open_pairs"]["pairs"][0]
                                    .clone();
                                let mut pairs = vec![];
                                for run in ["old-one", "old-two"] {
                                    let raw = serde_json::to_string(&pair)
                                        .unwrap()
                                        .replace(":\"run\"", &format!(":\"{run}\""));
                                    pairs.push(serde_json::from_str::<s::ReadPair>(&raw).unwrap());
                                }
                                serde_json::to_vec(&s::QueryResult::OpenPairs { pairs }).unwrap()
                            } else {
                                if matches!(fault, Fault::RepairPairs) {
                                    assert_eq!(filed_pairs, 2);
                                }
                                br#"{"open_pairs":{"pairs":[]}}"#.to_vec()
                            }
                        }
                        s::ReadSelector::PlanCurrent {} => {
                            queries.push("summary");
                            br#"{"plan_current":{"current":null,"next_plan_id":"40","next_step_id":"50"}}"#.to_vec()
                        }
                        _ => panic!("unexpected selector {selector:?}"),
                    };
                    snapshot += 1;
                    if matches!(fault, Fault::RefusePrompt) {
                        s::StateControlAnswer::ControlRefused {
                            code: s::ControlRefusalCode::Read(s::ReadRefusalCode::InvalidSelector),
                            detail: "fixture refusal".into(),
                        }
                    } else {
                        super::super::read_tests::page(snapshot, 0, raw.len(), &raw)
                    }
                }
                s::StateControl::Release { snapshot_id } => {
                    releases += 1;
                    s::StateControlAnswer::Released {
                        snapshot_id: *snapshot_id,
                    }
                }
                _ => panic!("unexpected control {frame:?}"),
            };
            let now = fx.input.run.clock.now().unwrap();
            let response = s::StateFrame::ControlAnswer {
                common: request.common().clone(),
                frame: answer,
                observation: operation(&request, operation_id, now),
            };
            operation_id += 1;
            if matches!(fault, Fault::HoldPromptRelease)
                && matches!(frame, s::StateControl::Release { .. })
                && releases == 1
            {
                held_release = Some((response, 10));
                continue;
            }
            fx.input
                .run
                .state
                .write_all(&s::encode_state_line(&response, s::FrameDirection::Response).unwrap())
                .unwrap();
        }
        if actual_spu {
            continue;
        }
        match recv(
            decoder.as_fd().as_raw_fd(),
            &mut bytes,
            MsgFlags::MSG_DONTWAIT,
        ) {
            Ok(n) => {
                let request: TokenDirective = serde_json::from_slice(&bytes[..n]).unwrap();
                let TokenDirective::Open { messages, .. } = request else {
                    panic!("unexpected decode")
                };
                assert_eq!(queries, ["prompt"]);
                assert_eq!(releases, 1);
                assert!(boot.context.boot_head.is_some());
                assert_eq!(messages.len(), 1);
                opens += 1;
                let rendered = "<system>Read caf\u{e9}.</system>";
                let generation_limit = if matches!(fault, Fault::ZeroGeneration) {
                    0
                } else {
                    16
                };
                let answer = TokenAnswer::Opened(Opened { session: SessionId("session".into()), prefix_tokens: 3, capacity: 16384, generation_limit, terminal_tokens: 1,
                    request: serde_json::value::to_raw_value(&serde_json::json!({"rendered":rendered,"template":"fixture"})).unwrap(),
                    measurement: serde_json::value::to_raw_value(&serde_json::json!({"model":"fixture","weights_hash":"fixture","prefix_token_ids":[41,7,89],"blocks":[{"label":"identity-prefix","start":0,"end":rendered.len()}]})).unwrap() });
                let raw = serde_json::to_vec(&answer).unwrap();
                assert_eq!(
                    send(
                        decoder.as_fd().as_raw_fd(),
                        &raw,
                        MsgFlags::MSG_DONTWAIT | MsgFlags::MSG_NOSIGNAL
                    ),
                    Ok(raw.len())
                );
            }
            Err(Errno::EAGAIN) => {}
            other => panic!("decode receive: {other:?}"),
        }
    }
    panic!(
        "boot composition stalled at {:?}: {:?}",
        boot.stage(),
        boot.stopped()
    );
}

#[test]
fn boot_positive_composition_stops_before_pair_repair_summary_and_install() {
    circuit(Fault::ZeroGeneration);
}
#[test]
fn boot_composition_drives_the_whole_recorded_sequence_once() {
    circuit(Fault::None);
}
#[test]
fn boot_composition_template_rejection_prevents_prompt_and_decode() {
    circuit(Fault::RejectTemplate);
}
#[test]
fn boot_composition_prompt_refusal_prevents_decoder_open() {
    circuit(Fault::RefusePrompt);
}
#[test]
fn boot_composition_failure_between_summary_and_install_retains_actual_boot() {
    circuit(Fault::StopBeforeInstall);
}
#[test]
fn boot_composition_late_finish_returns_entire_owner() {
    circuit(Fault::ExpireBeforeFinish);
}

#[test]
fn boot_composition_repairs_two_prior_calls_before_summary_and_install() {
    circuit(Fault::RepairPairs);
}

#[test]
fn boot_composition_waits_for_actual_prompt_release_before_open() {
    circuit(Fault::HoldPromptRelease);
}
