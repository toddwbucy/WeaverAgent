//! Real-model source generation and deterministic replay interruption.
use super::*;
use crate::channel::{DecodeChannel, DecodeReply, OrganChannel};
use std::process::{Command, Stdio};

struct Child(std::process::Child);
impl Drop for Child {
    fn drop(&mut self) {
        let _ = self.0.kill();
        let _ = self.0.wait();
    }
}
fn binary() -> PathBuf {
    std::env::current_exe()
        .unwrap()
        .parent()
        .unwrap()
        .parent()
        .unwrap()
        .join("weaver-spu")
}
pub(super) fn prepare(root: &std::path::Path, path: &std::path::Path) {
    let quote = |s: &str| format!("'{}'", s.replace('\'', "'\\''"));
    assert!(binary().is_file(), "build exact private SPU with cuda,gguf");
    std::fs::write(
        path,
        format!(
            "#!/bin/sh\necho $$ > {}\ntouch {}\nexec {}\n",
            quote(root.join("diagnostic-real-spu-pid").to_str().unwrap()),
            quote(root.join("diagnostic-spawned").to_str().unwrap()),
            quote(binary().to_str().unwrap())
        ),
    )
    .unwrap();
}
/// The scenario's model, by the SPU tests' fixture convention (SPU Spec 1.2):
/// `WEAVER_ARTIFACT_QWEN25_SAFETENSORS` names the native directory and
/// `WEAVER_TEST_GGUF` the GGUF file. Unset and absent skips; named and absent
/// fails, so a runner that asked for the real model is told it did not run.
pub(crate) fn artifact(mode: &str) -> Option<PathBuf> {
    let (variable, fallback, native) = if mode.ends_with("-native") {
        (
            "WEAVER_ARTIFACT_QWEN25_SAFETENSORS",
            "/bulk-store/models/Qwen--Qwen2.5-0.5B-Instruct",
            true,
        )
    } else {
        (
            "WEAVER_TEST_GGUF",
            "/opt/weaver/models/qwen2.5-0.5b-instruct-q6_k.gguf",
            false,
        )
    };
    let resolve = |path: PathBuf| {
        let file = if native {
            path.join("model.safetensors")
        } else {
            path
        };
        file.is_file().then_some(file)
    };
    match std::env::var_os(variable) {
        Some(named) => {
            let found = resolve(PathBuf::from(&named));
            assert!(
                found.is_some(),
                "{variable} names an unavailable artifact: {}",
                PathBuf::from(named).display()
            );
            found
        }
        None => resolve(PathBuf::from(fallback)),
    }
}
pub(super) fn source(
    root: &std::path::Path,
    mode: &str,
    instruction: &mut weaver_types::SpuInstruction,
    end: Instant,
) -> (serde_json::Value, serde_json::Value) {
    instruction.decoder.model_binding = weaver_types::ModelBinding {
        artifact: weaver_types::ArtifactRef(
            artifact(mode)
                .expect("the scenario skips before spawning without its fixture")
                .to_str()
                .unwrap()
                .into(),
        ),
        devices: vec![weaver_types::DeviceOrdinal(0)],
    };
    instruction.decoder.tunable_values = [
        ("max-tokens-per-turn".into(), 16.0),
        ("context-capacity".into(), 1024.0),
        ("seed".into(), 37.0),
    ]
    .into_iter()
    .collect();
    instruction.decoder.field_election = Some(weaver_types::FieldElection { depth: 40 });
    instruction.decoder.column_permission = false;
    instruction.decoder.residual_readout_election = false;
    let (life, far_life) = OrganChannel::pair().unwrap();
    let (decode, far_decode) = DecodeChannel::pair().unwrap();
    let high = |fd| {
        let n = unsafe { nix::libc::fcntl(fd, nix::libc::F_DUPFD_CLOEXEC, 100) };
        assert!(n >= 100);
        unsafe { OwnedFd::from_raw_fd(n) }
    };
    let l = high(far_life.as_fd().as_raw_fd());
    let d = high(far_decode.as_fd().as_raw_fd());
    let lf = l.as_raw_fd();
    let df = d.as_raw_fd();
    let mut command = Command::new(binary());
    command
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(std::fs::File::create(root.join("baseline-spu.log")).unwrap());
    // SAFETY: high duplicates remain owned through spawn; only fd calls follow fork.
    unsafe {
        command.pre_exec(move || {
            if nix::libc::dup2(lf, 3) < 0 || nix::libc::dup2(df, 4) < 0 {
                return Err(std::io::Error::last_os_error());
            }
            Ok(())
        });
    }
    let mut child = Child(command.spawn().unwrap());
    drop((l, d, far_life, far_decode));
    let mut wait = |fd: std::os::fd::BorrowedFd<'_>, flags| {
        let mut fds = [nix::poll::PollFd::new(fd, flags)];
        loop {
            if Instant::now() >= end {
                return Err(crate::ChannelFault::Closed);
            }
            nix::poll::poll(&mut fds, 100_u16).map_err(|_| crate::ChannelFault::Closed)?;
            if fds[0].revents().is_some_and(|f| !f.is_empty()) {
                return Ok(());
            }
        }
    };
    let ask = |ordinal, directive| OrganEnvelope {
        exchange: ExchangeId {
            opener: Opener::Harness,
            ordinal,
        },
        position: Position::Open,
        payload: Payload::Directive(directive),
    };
    life.send_managed(
        &ask(
            1,
            LifecycleDirective::Admit {
                instruction: instruction.clone(),
            },
        ),
        &mut wait,
    )
    .unwrap();
    assert_eq!(
        life.recv_managed(&mut wait).unwrap().payload,
        Payload::Answer(LifecycleAnswer::Admitted)
    );
    decode
        .send_managed(
            &TokenDirective::Open {
                session: weaver_types::SessionId("session".into()),
                messages: vec![weaver_traits::Message {
                    role: weaver_traits::Role::System,
                    content: vec![weaver_traits::ContentBlock::Text {
                        text: "Read caf\u{e9}.".into(),
                    }],
                }],
                column_ask: false,
            },
            &mut wait,
        )
        .unwrap();
    assert!(matches!(
        decode.recv_managed(&mut wait).unwrap(),
        DecodeReply::Answer(TokenAnswer::Opened(_))
    ));
    decode
        .send_managed(
            &TokenDirective::AppendAndGenerate {
                turn: weaver_types::TurnKey("baseline".into()),
                delta: vec![weaver_traits::Message {
                    role: weaver_traits::Role::User,
                    content: vec![weaver_traits::ContentBlock::Text {
                        text: "Count from one to ten.".into(),
                    }],
                }],
                tool_schemas: vec![],
            },
            &mut wait,
        )
        .unwrap();
    let generation = loop {
        match decode.recv_managed(&mut wait).unwrap() {
            DecodeReply::Answer(TokenAnswer::Generated(g)) => break g,
            DecodeReply::Answer(TokenAnswer::Token { .. } | TokenAnswer::Field { .. }) => (),
            other => panic!("baseline answer: {other:?}"),
        }
    };
    let request = serde_json::from_str(generation.request.get()).unwrap();
    let measurement: serde_json::Value =
        serde_json::from_str(generation.measurement.get()).unwrap();
    assert!(!measurement["output_tokens"].as_array().unwrap().is_empty());
    std::fs::write(
        root.join("baseline-generation.json"),
        serde_json::to_vec_pretty(&generation).unwrap(),
    )
    .unwrap();
    drop(decode);
    life.send_managed(&ask(2, LifecycleDirective::Release), &mut wait)
        .unwrap();
    assert_eq!(
        life.recv_managed(&mut wait).unwrap().payload,
        Payload::Answer(LifecycleAnswer::Released)
    );
    drop(life);
    let (status, expired) = crate::test_child::wait_until(&mut child.0, end).unwrap();
    assert!(!expired && status.success());
    (request, measurement)
}
pub(super) fn pause(root: &std::path::Path, end: Instant) -> i32 {
    let pid: i32 = std::fs::read_to_string(root.join("diagnostic-real-spu-pid"))
        .unwrap()
        .trim()
        .parse()
        .unwrap();
    assert_eq!(unsafe { nix::libc::kill(pid, nix::libc::SIGSTOP) }, 0);
    loop {
        let status = std::fs::read_to_string(format!("/proc/{pid}/status")).unwrap();
        if status
            .lines()
            .any(|s| s.starts_with("State:") && s.contains("T (stopped)"))
        {
            break;
        }
        assert!(Instant::now() < end);
        std::thread::sleep(Duration::from_millis(1));
    }
    pid
}
pub(super) fn resume(pid: i32) {
    assert_eq!(unsafe { nix::libc::kill(pid, nix::libc::SIGCONT) }, 0);
}
