//! Actual forked residency/decoder peer, with scripted model behavior.
use super::*;
use crate::channel::OrganChannel;
use crate::flow::clock::{ReadyFd, WaitInterest, WaitOutcome, testing::FakePumpIo};
use crate::flow::pump::LifecyclePeer;
use nix::errno::Errno;
use nix::poll::PollFlags;
use nix::sys::socket::{MsgFlags, recv, send};
use std::os::fd::{AsRawFd, FromRawFd};
use std::os::unix::fs::PermissionsExt;
use std::time::{Duration, Instant};
use weaver_types::{ExchangeId, Opener, OrganEnvelope, TokenAnswer, TokenDirective};

struct CircuitIo<'a>(&'a mut FakePumpIo);
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

macro_rules! packet {
    ($fd:expr, $value:expr $(,)?) => {{
        let bytes = serde_json::to_vec($value).unwrap();
        assert_eq!(send($fd, &bytes, MsgFlags::MSG_NOSIGNAL), Ok(bytes.len()));
    }};
}

#[test]
fn residency_composition_child() {
    let Ok(mode) = std::env::var("WEAVER_RESIDENCY_COMPOSITION_CHILD") else {
        return;
    };
    // SAFETY: these are the two ends mapped by this child's actual fork producer.
    let lifecycle = unsafe { std::os::fd::OwnedFd::from_raw_fd(3) };
    let decode = unsafe { std::os::fd::OwnedFd::from_raw_fd(4) };
    let mut admitted = false;
    let mut opened = false;
    let mut bytes = [0u8; 65536];
    if mode == "early" {
        packet!(
            3,
            &OrganEnvelope {
                exchange: ExchangeId {
                    opener: Opener::Harness,
                    ordinal: 1,
                },
                position: Position::Close,
                payload: weaver_types::Payload::Answer(LifecycleAnswer::Admitted),
            },
        );
    }
    if mode == "eof" {
        return;
    }
    let deadline = Instant::now() + Duration::from_secs(10);
    while Instant::now() < deadline {
        match recv(lifecycle.as_raw_fd(), &mut bytes, MsgFlags::MSG_DONTWAIT) {
            Ok(0) => return,
            Ok(n) => {
                let request: OrganEnvelope = serde_json::from_slice(&bytes[..n]).unwrap();
                if matches!(
                    request.payload,
                    weaver_types::Payload::Directive(LifecycleDirective::Release)
                ) {
                    assert!(admitted);
                    assert_eq!(
                        recv(decode.as_raw_fd(), &mut bytes, MsgFlags::MSG_DONTWAIT),
                        Ok(0)
                    );
                    packet!(
                        lifecycle.as_raw_fd(),
                        &OrganEnvelope {
                            exchange: request.exchange,
                            position: Position::Close,
                            payload: weaver_types::Payload::Answer(LifecycleAnswer::Released),
                        }
                    );
                    return;
                }
                let weaver_types::Payload::Directive(LifecycleDirective::Admit { instruction }) =
                    request.payload
                else {
                    panic!("unexpected residency request")
                };
                assert!(!admitted);
                assert_eq!(instruction, instruction_fixture());
                assert_eq!(request.position, Position::Open);
                assert_eq!(request.exchange.opener, Opener::Harness);
                if mode == "delay" || mode == "full" {
                    for _ in 0..20 {
                        assert_eq!(
                            recv(decode.as_raw_fd(), &mut bytes, MsgFlags::MSG_DONTWAIT),
                            Err(Errno::EAGAIN)
                        );
                        std::thread::sleep(Duration::from_millis(1));
                    }
                }
                let mut response = OrganEnvelope {
                    exchange: request.exchange,
                    position: Position::Close,
                    payload: if mode == "cleanup-release-refusal" {
                        weaver_types::Payload::Refusal(weaver_types::LifecycleRefusal::NoResidency)
                    } else if matches!(mode.as_str(), "refuse" | "cleanup-refuse") {
                        weaver_types::Payload::Refusal(
                            weaver_types::LifecycleRefusal::DeviceCannotAdmit,
                        )
                    } else {
                        weaver_types::Payload::Answer(LifecycleAnswer::Admitted)
                    },
                };
                if mode == "wrong" {
                    response.exchange.ordinal += 1;
                }
                if mode == "position" {
                    response.position = Position::Open;
                }
                if mode == "opener" {
                    response.exchange.opener = Opener::Admin;
                }
                if mode == "arm" {
                    response.payload = weaver_types::Payload::Answer(LifecycleAnswer::Released);
                }
                if mode != "early" {
                    packet!(lifecycle.as_raw_fd(), &response);
                }
                admitted = mode != "refuse";
            }
            Err(Errno::EAGAIN) => {}
            other => panic!("residency child receive {other:?}"),
        }
        match recv(decode.as_raw_fd(), &mut bytes, MsgFlags::MSG_DONTWAIT) {
            Ok(0) if mode == "cleanup-ready" => {}
            Ok(0) => return,
            Ok(n) => {
                assert!(admitted);
                assert!(!opened);
                let request: TokenDirective = serde_json::from_slice(&bytes[..n]).unwrap();
                let TokenDirective::Open {
                    session, messages, ..
                } = request
                else {
                    panic!("unexpected decode directive")
                };
                assert_eq!(messages.len(), 1);
                opened = true;
                let rendered = "<system>Read caf\u{e9}.</system>";
                let answer = TokenAnswer::Opened(weaver_types::Opened { session, prefix_tokens: 3, capacity: 16384,
                    generation_limit: 16, terminal_tokens: 1,
                    request: serde_json::value::to_raw_value(&serde_json::json!({"rendered":rendered,"template":"fixture"})).unwrap(),
                    measurement: serde_json::value::to_raw_value(&serde_json::json!({"model":"fixture","weights_hash":"fixture","prefix_token_ids":[41,7,89],"blocks":[{"label":"identity-prefix","start":0,"end":rendered.len()}]})).unwrap() });
                packet!(decode.as_raw_fd(), &answer);
            }
            Err(Errno::EAGAIN) => {}
            other => panic!("decode child receive {other:?}"),
        }
        std::thread::sleep(Duration::from_millis(1));
    }
    panic!("residency fixture watchdog");
}

fn instruction_fixture() -> SpuInstruction {
    SpuInstruction {
        classify: None,
        decoder: weaver_types::DecoderInstruction {
            model_binding: weaver_types::ModelBinding {
                artifact: weaver_types::ArtifactRef("fixture-spu".into()),
                devices: vec![weaver_types::DeviceOrdinal(0)],
            },
            residual_readout_election: false,
            field_election: None,
            surprisal_election: false,
            refeed_permission: false,
            column_permission: false,
            tunable_values: Default::default(),
        },
    }
}

struct BinaryFixture(PathBuf);
impl BinaryFixture {
    fn new(mode: &str) -> Self {
        static NEXT: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
        let id = NEXT.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        let path = std::env::temp_dir().join(format!(
            "weaver-residency-child-{}-{id}",
            std::process::id()
        ));
        let quote = |s: &str| format!("'{}'", s.replace('\'', "'\\''"));
        std::fs::write(&path, format!("#!/bin/sh\nexport WEAVER_RESIDENCY_COMPOSITION_CHILD={}\nexec {} --exact flow::context::residency_composition::tests::residency_composition_child --nocapture\n",quote(mode),quote(std::env::current_exe().unwrap().to_str().unwrap()))).unwrap();
        std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o700)).unwrap();
        Self(path)
    }
}
impl Drop for BinaryFixture {
    fn drop(&mut self) {
        let _ = std::fs::remove_file(&self.0);
    }
}

pub(in crate::flow::context) fn drive<'a>(
    startup: PreparedStartup<'a>,
    clock: &mut FakePumpIo,
    mode: &str,
    state: &std::os::unix::net::UnixStream,
    stopped: &mut Option<StoppedResidency>,
) -> Option<AdmittedStartup<'a>> {
    let binary = BinaryFixture::new(if mode == "cleanup-wrong" {
        "wrong"
    } else if mode.starts_with("cleanup-ready") {
        "cleanup-ready"
    } else {
        mode
    });
    let mut owner = ResidencyComposition::new(
        startup,
        ResidencyRequest {
            binary: binary.0.clone(),
            arguments: vec![],
            instruction: instruction_fixture(),
        },
    );
    if mode == "cleanup-before-fork" {
        state.shutdown(std::net::Shutdown::Write).unwrap();
        for _ in 0..32 {
            let _ = owner.context.pump_once(&mut PumpPorts {
                lifecycle: LifecyclePorts::default(),
                gate: None,
                decode: None,
                label: None,
                io: &mut CircuitIo(clock),
            });
            if owner.context.failure().is_some() {
                break;
            }
        }
        assert!(owner.context.failure().is_some());
    }
    let origin = owner.context.origin_ns();
    let enter = owner.context.deadlines.get(PumpDeadline::Enter).unwrap();
    let (admin, peer) = OrganChannel::pair().unwrap();
    let mut admin = LifecyclePeer::from_channel(admin).unwrap();
    let mut staged_stop = false;
    for _ in 0..3000 {
        if mode == "early" && owner.stage() == ResidencyStage::Admit && owner.send().is_none() {
            for _ in 0..3000 {
                let port = owner.resources.residency_mut().unwrap();
                port.try_receive().unwrap();
                if port.held().is_some() {
                    break;
                }
                std::thread::sleep(Duration::from_millis(1));
            }
            assert!(owner.resources.residency_mut().unwrap().held().is_some());
        }
        if mode == "stop" && owner.stage() == ResidencyStage::Admit && !staged_stop {
            // Exercise a real receive-first pump unit before the simultaneous
            // admin/readiness wake, so the next residency unit would send first.
            owner
                .context
                .pump_once(&mut PumpPorts {
                    lifecycle: LifecyclePorts {
                        listener: None,
                        peer: None,
                        residency: owner.resources.residency_mut(),
                        dial: None,
                        leave_peer: None,
                    },
                    gate: None,
                    decode: None,
                    label: None,
                    io: &mut CircuitIo(clock),
                })
                .unwrap();
            packet!(
                peer.as_fd().as_raw_fd(),
                &OrganEnvelope {
                    exchange: ExchangeId {
                        opener: Opener::Admin,
                        ordinal: 9,
                    },
                    position: Position::Open,
                    payload: weaver_types::Payload::Directive(LifecycleDirective::Stop {
                        cause: weaver_types::Cause { uid: 0 },
                    }),
                },
            );
            staged_stop = true;
        }
        let ports = LifecyclePorts {
            listener: None,
            peer: (mode == "stop").then_some(&mut admin),
            residency: None,
            dial: None,
            leave_peer: None,
        };
        // SAFETY: the caller's fixture guard retains/reaps children on this thread.
        let stage = unsafe { owner.advance(ports, &mut CircuitIo(clock)) };
        assert_eq!(owner.context.origin_ns(), origin);
        assert_eq!(
            owner.context.deadlines.get(PumpDeadline::Enter),
            Some(enter)
        );
        if mode == "stop"
            && let Some(send) = owner.send()
        {
            assert!(!send.sent());
        }
        if stage == ResidencyStage::Stopped {
            let (_, resources, account) = match owner.into_stopped() {
                Ok(x) => x,
                Err(_) => panic!("stopped custody refused"),
            };
            assert_eq!(
                resources.spu_child().is_some(),
                mode != "cleanup-before-fork"
            );
            if mode.starts_with("cleanup-") {
                *stopped = Some(account);
                return None;
            }
            assert_eq!(account.request.instruction, instruction_fixture());
            match mode {
                "refuse" => {
                    assert!(matches!(account.cause, ResidencyStop::Refused));
                    assert!(matches!(
                        account.reply.as_ref().unwrap().envelope().payload,
                        weaver_types::Payload::Refusal(
                            weaver_types::LifecycleRefusal::DeviceCannotAdmit
                        )
                    ));
                }
                "wrong" | "position" | "opener" | "arm" | "early" => {
                    assert!(matches!(account.cause, ResidencyStop::Run(_)));
                    assert!(!account.reply.as_ref().unwrap().raw_bytes().is_empty());
                    if mode == "early" {
                        assert!(!resources.residency_request().unwrap().send().sent());
                    }
                }
                "eof" => {
                    assert!(matches!(account.cause, ResidencyStop::Run(_)));
                    assert!(account.reply.is_none());
                }
                "stop" => {
                    assert!(matches!(account.cause, ResidencyStop::Interrupted));
                    assert!(!resources.residency_request().unwrap().send().sent());
                }
                _ => panic!("unexpected stop {:?}", account.cause),
            }
            return None;
        }
        if stage == ResidencyStage::Complete {
            if mode.starts_with("cleanup-ready") {
                state.shutdown(std::net::Shutdown::Write).unwrap();
                for _ in 0..32 {
                    let _ = owner.context.pump_once(&mut PumpPorts {
                        lifecycle: LifecyclePorts::default(),
                        gate: None,
                        decode: None,
                        label: None,
                        io: &mut CircuitIo(clock),
                    });
                    if owner.context.failure().is_some() {
                        break;
                    }
                }
                assert!(
                    owner.context.failure().is_some(),
                    "actual state EOF must latch"
                );
                continue;
            }
            assert!(matches!(mode, "ready" | "delay" | "full" | "expiry"));
            assert!(owner.send().unwrap().sent());
            let sent_at = owner.send().unwrap().sent_at().unwrap();
            assert!(sent_at >= origin && sent_at < enter.deadline_ns());
            if mode == "expiry" {
                clock.advance_to(enter.deadline_ns()).unwrap();
                let stopped = match owner.finish(LifecyclePorts::default(), clock) {
                    Err(x) => x,
                    Ok(_) => panic!("expired residency granted boot"),
                };
                let (_, _, account) = match stopped.into_stopped() {
                    Ok(x) => x,
                    Err(_) => panic!("expired custody refused"),
                };
                assert!(matches!(
                    account.cause,
                    ResidencyStop::Run(RunFailure::Wait { .. })
                ));
                assert!(account.reply.is_some());
                return None;
            }
            return Some(match owner.finish(LifecyclePorts::default(), clock) {
                Ok(x) => x,
                Err(x) => panic!("live finish refused {:?}", x.stopped()),
            });
        }
        std::thread::sleep(Duration::from_millis(1));
    }
    panic!("residency stalled {:?}", owner.stopped());
}

macro_rules! scenario {
    ($name:ident, $mode:literal) => {
        #[test]
        fn $name() {
            super::super::startup_composition::tests::residency($mode);
        }
    };
}
scenario!(
    residency_composition_connects_startup_to_real_decoder_boot,
    "full"
);
scenario!(
    residency_composition_delayed_admit_prevents_early_decode,
    "delay"
);
scenario!(residency_composition_preserves_typed_refusal, "refuse");
#[test]
fn residency_composition_rejects_foreign_exchange_and_retains_wire() {
    for mode in ["wrong", "position", "opener"] {
        super::super::startup_composition::tests::residency(mode);
    }
}
scenario!(residency_composition_rejects_wrong_answer_arm, "arm");
scenario!(residency_composition_rejects_answer_before_send, "early");
scenario!(residency_composition_eof_never_grants_boot, "eof");
scenario!(
    residency_composition_final_expiry_keeps_actual_reply,
    "expiry"
);
scenario!(
    residency_composition_observed_stop_prevents_first_admit_send,
    "stop"
);
