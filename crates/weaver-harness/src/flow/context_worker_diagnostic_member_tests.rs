//! SQLite preload and worker replay with scripted or real GGUF/native SPU peers.
use super::*;
use std::process::{Command, Stdio};

struct Member(std::process::Child);
impl Drop for Member {
    fn drop(&mut self) {
        let _ = self.0.kill();
        let _ = self.0.wait();
    }
}

#[allow(clippy::too_many_arguments)]
pub(super) fn control(
    root: &std::path::Path,
    mode: &str,
    child: &mut std::process::Child,
    mut envelope: EnterEnvelope,
    sink: OwnedFd,
    state: OwnedFd,
    reader: &mut UnixStream,
    member: &mut UnixStream,
    end: Instant,
) {
    let election = weaver_trace::Election {
        all_kinds: true,
        keys: vec![
            weaver_trace::ElectedKind {
                kind: "model.request".into(),
                paths: ["rendered", "template", "sampling"]
                    .map(str::to_owned)
                    .to_vec(),
            },
            weaver_trace::ElectedKind {
                kind: "model.measurement".into(),
                paths: ["input_tokens", "output_tokens", "model", "weights_hash"]
                    .map(str::to_owned)
                    .to_vec(),
            },
        ],
    };
    let EnterContent::Directive(EnterDirective::Enter { payload }) = &mut envelope.payload;
    payload.binding = EnterBinding::Diagnostic {
        state_election: serde_json::from_value(
            serde_json::json!({"all-kinds":election.all_kinds,"keys":election.keys}),
        )
        .unwrap(),
    };
    let model = mode.contains("-real-model-");
    let actual_source =
        model.then(|| real_model::source(root, mode, &mut payload.spu_instruction, end));
    let turn = if model {
        "turn-".repeat(6 * 1024)
    } else {
        "t-1".into()
    };
    if model {
        interrupt::record_pressure::prepare(&sink);
    }
    let room = root.join("diagnostic-member");
    std::fs::create_dir(&room).unwrap();
    let door = room.join("preload.sock");
    let binary = std::env::current_exe()
        .unwrap()
        .parent()
        .unwrap()
        .parent()
        .unwrap()
        .join("weaver-state");
    assert!(binary.is_file(), "build the private SQLite member first");
    // A high duplicate keeps descriptor placement independent of the test's fds.
    let high = unsafe { nix::libc::fcntl(member.as_raw_fd(), nix::libc::F_DUPFD_CLOEXEC, 100) };
    assert!(high >= 100);
    // SAFETY: the fcntl result is a fresh exclusive descriptor.
    let inherited = unsafe { OwnedFd::from_raw_fd(high) };
    let mut command = Command::new(binary);
    command
        .arg(&room)
        .arg(&door)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(std::fs::File::create(room.join("stderr")).unwrap());
    // SAFETY: only async-signal-safe descriptor calls run between fork and exec.
    unsafe {
        command.pre_exec(move || {
            if nix::libc::dup2(high, 3) < 0 {
                return Err(std::io::Error::last_os_error());
            }
            nix::libc::close(4);
            nix::libc::close(9);
            Ok(())
        });
    }
    member.set_nonblocking(false).unwrap();
    let mut actual = Member(command.spawn().unwrap());
    drop(inherited);
    let peer = dial(&root.join("admin.sock"));
    send_enter(
        &peer,
        &envelope.encode().unwrap(),
        &[sink.as_raw_fd(), state.as_raw_fd()],
    );
    drop((sink, state));
    while !door.exists() {
        assert!(
            Instant::now() < end && actual.0.try_wait().unwrap().is_none(),
            "member failed: {}",
            std::fs::read_to_string(room.join("stderr")).unwrap()
        );
        std::thread::sleep(Duration::from_millis(1));
    }
    send(&peer, 981, LifecycleDirective::Observe);
    let observation = ObservationEnvelope::decode(&receive(&peer, end)).unwrap();
    assert_eq!(observation.exchange.ordinal, 981);
    assert!(matches!(
        observation.payload,
        ObservationContent::Answer(ObservationAnswer::InTransition { .. })
    ));
    assert!(
        !root.join("diagnostic-spawned").exists(),
        "unsealed identity grants no SPU"
    );
    let mut preload = UnixStream::connect(&door).unwrap();
    preload
        .write_all(weaver_trace::opener("session", &election).as_bytes())
        .unwrap();
    let mut source = [
        serde_json::json!({"session":"session","run":"source","sequence":"1","kind":"message.system","payload":{"role":"system","content":[{"type":"text","text":"Read caf\u{e9}."}]}}),
        serde_json::json!({"session":"session","run":"source","turn":"t-1","sequence":"2","kind":"model.request","payload":{"rendered":"hi","template":"tmpl","sampling":{"seed":37}}}),
        serde_json::json!({"session":"session","run":"source","turn":"t-1","sequence":"3","kind":"model.measurement","payload":{"input_tokens":[1,2],"output_tokens":[3],"model":"art","weights_hash":"h"}}),
    ];
    source[1]["turn"] = serde_json::json!(turn);
    source[2]["turn"] = serde_json::json!(turn);
    if let Some((request, measurement)) = actual_source {
        source[1]["payload"] = request;
        source[2]["payload"] = measurement;
    }
    for event in source {
        preload
            .write_all(
                weaver_trace::distill(&event.to_string(), &election)
                    .unwrap()
                    .as_bytes(),
            )
            .unwrap();
    }
    preload.write_all(b"{}\n").unwrap();
    drop(preload);
    let ready: OrganEnvelope = serde_json::from_slice(&receive(&peer, end)).unwrap();
    assert_eq!(ready.exchange, envelope.exchange);
    assert_eq!(ready.payload, Payload::Answer(LifecycleAnswer::Ready));
    let mut prefix = String::new();
    let mut drain = None;
    let paused = if model {
        prefix =
            interrupt::closing::hold_after(reader, end, "turn.started", "\"turn\":\"turn-turn-");
        Some(real_model::pause(root, end))
    } else {
        None
    };
    while !model && !root.join("diagnostic-refeed").exists() {
        assert!(Instant::now() < end && child.try_wait().unwrap().is_none());
        std::thread::sleep(Duration::from_millis(1));
    }
    send(
        &peer,
        982,
        LifecycleDirective::Stop {
            cause: Cause { uid: 41 },
        },
    );
    if let Some(pid) = paused {
        send(&peer, 984, LifecycleDirective::Observe);
        let observed = ObservationEnvelope::decode(&receive(&peer, end)).unwrap();
        assert_eq!(observed.exchange.ordinal, 984);
        assert!(matches!(
            observed.payload,
            ObservationContent::Answer(ObservationAnswer::State {
                state: AgentState::Active,
                ..
            })
        ));
        real_model::resume(pid);
        let mut stream = reader.try_clone().unwrap();
        stream.set_nonblocking(false).unwrap();
        stream
            .set_read_timeout(Some(Duration::from_secs(70)))
            .unwrap();
        drain = Some(std::thread::spawn(move || {
            let mut bytes = String::new();
            stream.read_to_string(&mut bytes).unwrap();
            bytes
        }));
    }
    let stopped: OrganEnvelope = serde_json::from_slice(&receive(&peer, end)).unwrap();
    assert_eq!(stopped.exchange.ordinal, 982);
    assert_eq!(
        stopped.payload,
        Payload::Answer(LifecycleAnswer::TurnAborted {
            turn: weaver_types::TurnKey(turn.clone())
        })
    );
    send(
        &peer,
        983,
        LifecycleDirective::Leave {
            cause: Cause { uid: 71 },
            forced: false,
            rollback: false,
        },
    );
    let left: OrganEnvelope = serde_json::from_slice(&receive(&peer, end)).unwrap();
    assert_eq!(left.exchange.ordinal, 983);
    assert_eq!(
        left.payload,
        Payload::Answer(LifecycleAnswer::Left {
            no_state: true,
            forced: false,
            save_point: None
        })
    );
    let (status, expired) = crate::test_child::wait_until(child, end).unwrap();
    assert!(!expired && status.success());
    member.shutdown(std::net::Shutdown::Both).unwrap();
    let (status, expired) = crate::test_child::wait_until(&mut actual.0, end).unwrap();
    assert!(
        !expired && status.success(),
        "member: {}",
        std::fs::read_to_string(room.join("stderr")).unwrap()
    );
    reader.set_nonblocking(false).unwrap();
    reader
        .set_read_timeout(Some(Duration::from_secs(2)))
        .unwrap();
    let mut bytes = prefix;
    if let Some(drain) = drain {
        bytes.push_str(&drain.join().unwrap());
    } else {
        reader.read_to_string(&mut bytes).unwrap();
    }
    if let Some(destination) = std::env::var_os("WEAVER_DISPATCH_EVIDENCE") {
        let destination = std::path::PathBuf::from(destination).join(mode);
        std::fs::create_dir_all(&destination).unwrap();
        std::fs::write(destination.join("trace.jsonl"), &bytes).unwrap();
        std::fs::copy(room.join("stderr"), destination.join("member-stderr.log")).unwrap();
        if model {
            for file in ["baseline-generation.json", "baseline-spu.log"] {
                std::fs::copy(root.join(file), destination.join(file)).unwrap();
            }
        }
    }
    let rows: Vec<serde_json::Value> = bytes
        .lines()
        .map(|line| serde_json::from_str(line).unwrap())
        .collect();
    assert_eq!(rows[0]["kind"], "replay.opened");
    assert_eq!(rows.iter().filter(|r| r["kind"] == "recall").count(), 1);
    let close: Vec<_> = rows.iter().filter(|r| r["kind"] == "turn.closed").collect();
    assert_eq!(close.len(), 1);
    assert_eq!(close[0]["payload"]["reason"], "directive");
    assert!(
        !rows
            .iter()
            .any(|r| r["kind"] == "replay.closed" || !model && r["kind"] == "model.measurement")
    );
    if model {
        assert!(rows.iter().any(|r| r["kind"] == "model.field"));
        let outputs = rows
            .iter()
            .filter(|r| r["kind"] == "model.measurement")
            .count();
        assert!(
            outputs <= 1,
            "complete race records one truthful generation"
        );
        println!(
            "real model {mode}: fields={}, complete_generation={outputs}",
            rows.iter().filter(|r| r["kind"] == "model.field").count()
        );
        let pid = std::fs::read_to_string(root.join("diagnostic-real-spu-pid")).unwrap();
        assert!(!std::path::Path::new(&format!("/proc/{}", pid.trim())).exists());
    } else {
        assert!(root.join("diagnostic-released").exists());
    }
}
