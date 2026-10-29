use super::*;
use nix::sys::socket::{AddressFamily, MsgFlags, SockFlag, SockType, recv, send, socketpair};
use serde_json::{Value, json};
use std::os::fd::AsRawFd;
use std::os::unix::fs::PermissionsExt;

fn document(task: &str) -> Vec<u8> {
    serde_json::to_vec(&json!({"format":"state-plan-update/1","expected":null,
        "operation":{"authorize":{"task":task,"categories":[],"groups":[],
            "review":{"setup_id":"review","sha256":"2".repeat(64)}}}}))
    .unwrap()
}

#[test]
fn root_plan_cause_is_checked_even_with_effective_root() {
    for (root, cause) in [(false, 0), (false, 1000), (true, 1000)] {
        assert_eq!(
            authorize(root, Ok(Cause { uid: cause })),
            Err(LifecycleRefusal::Unauthorized)
        );
    }
    assert_eq!(authorize(true, Ok(Cause { uid: 0 })), Ok(Cause { uid: 0 }));
    assert_eq!(
        authorize(true, Err(LifecycleRefusal::Malformed)),
        Err(LifecycleRefusal::Malformed)
    );
}

#[test]
fn root_plan_retains_exact_utf8_and_hashes_the_same_bytes() {
    let mut bytes = b" \n".to_vec();
    bytes.extend(document("OPEN the file."));
    bytes.extend(b" \n");
    let prepared = prepare_bytes(bytes.clone(), Cause { uid: 0 }).unwrap();
    let Payload::Directive(LifecycleDirective::PlanUpdate {
        document,
        document_sha256,
        cause,
    }) = &prepared.request.payload
    else {
        panic!()
    };
    assert_eq!(document.as_bytes(), bytes);
    assert_eq!(
        document_sha256.as_str(),
        format!("{:x}", Sha256::digest(&bytes))
    );
    assert_eq!(cause.uid, 0);
    assert_eq!(
        decode_strict::<OrganEnvelope>(std::str::from_utf8(&prepared.wire).unwrap()).unwrap(),
        prepared.request
    );
}

#[test]
fn root_plan_bounds_complete_escaped_envelope_and_sends_no_prefix() {
    let bytes = document("");
    // An empty task is invalid, so compute the exact fit from one byte of content.
    let base = prepare_bytes(document("x"), Cause { uid: 0 }).unwrap();
    let task = "x".repeat(MAX_ENVELOPE_BYTES - base.wire.len() + 1);
    let exact = prepare_bytes(document(&task), Cause { uid: 0 }).unwrap();
    assert_eq!(exact.wire.len(), MAX_ENVELOPE_BYTES);
    assert!(matches!(
        prepare_bytes(document(&(task + "x")), Cause { uid: 0 }),
        Err(LifecycleRefusal::PlanUpdateRefused {
            case: PlanUpdateCase::MalformedPlan,
            ..
        })
    ));
    let escaped = document(&"\"".repeat(20_000));
    assert!(escaped.len() < MAX_ENVELOPE_BYTES);
    assert!(prepare_bytes(escaped, Cause { uid: 0 }).is_err());
    assert!(prepare_bytes(bytes, Cause { uid: 0 }).is_err());
}

#[test]
fn root_plan_rejects_syntax_duplicates_unknown_fields_and_bad_utf8() {
    for bytes in [
        b"{".to_vec(),
        vec![0xff],
        b"[\"state-plan-update/1\",null,{}]".to_vec(),
        String::from_utf8(document("OPEN the file."))
            .unwrap()
            .replace("\"expected\":null", "\"expected\":null,\"expected\":null")
            .into_bytes(),
        String::from_utf8(document("OPEN the file."))
            .unwrap()
            .replacen('{', "{\"extra\":0,", 1)
            .into_bytes(),
    ] {
        assert!(matches!(
            prepare_bytes(bytes, Cause { uid: 0 }),
            Err(LifecycleRefusal::PlanUpdateRefused {
                case: PlanUpdateCase::MalformedPlan,
                read_code: None,
                ..
            })
        ));
    }
}

#[test]
#[ignore = "requires actual root-owned fixed-file custody; run the compiled test as root"]
fn root_plan_fixed_file_checks_opened_descriptor() {
    assert!(nix::unistd::geteuid().is_root());
    let temp = crate::scratch::Scratch(std::env::temp_dir().join(format!(
        "root-plan-file-{}",
        crate::random::random_hex(12).unwrap()
    )));
    std::fs::create_dir(&temp).unwrap();
    let root = File::open(temp.as_ref()).unwrap();
    let path = temp.as_ref().join("plan-update.json");
    assert_eq!(
        prepare(root.as_fd(), Cause { uid: 0 }).err(),
        Some(unsafe_file())
    );
    std::fs::write(&path, document("OPEN the file.")).unwrap();
    std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o600)).unwrap();
    assert!(prepare(root.as_fd(), Cause { uid: 0 }).is_ok());
    for mode in [0o620, 0o602, 0o666] {
        std::fs::set_permissions(&path, std::fs::Permissions::from_mode(mode)).unwrap();
        assert_eq!(
            prepare(root.as_fd(), Cause { uid: 0 }).err(),
            Some(unsafe_file())
        );
    }
    std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o600)).unwrap();
    nix::unistd::chown(&path, Some(nix::unistd::Uid::from_raw(65534)), None).unwrap();
    assert_eq!(
        prepare(root.as_fd(), Cause { uid: 0 }).err(),
        Some(unsafe_file())
    );
    nix::unistd::chown(&path, Some(nix::unistd::Uid::from_raw(0)), None).unwrap();
    let original = temp.as_ref().join("original");
    std::fs::rename(&path, &original).unwrap();
    std::os::unix::fs::symlink(&original, &path).unwrap();
    assert_eq!(
        prepare(root.as_fd(), Cause { uid: 0 }).err(),
        Some(unsafe_file())
    );
    std::fs::remove_file(&path).unwrap();
    std::fs::create_dir(&path).unwrap();
    assert_eq!(
        prepare(root.as_fd(), Cause { uid: 0 }).err(),
        Some(unsafe_file())
    );
    std::fs::remove_dir(&path).unwrap();
    nix::unistd::mkfifo(&path, nix::sys::stat::Mode::S_IRUSR).unwrap();
    assert_eq!(
        prepare(root.as_fd(), Cause { uid: 0 }).err(),
        Some(unsafe_file())
    );
    std::fs::remove_file(&path).unwrap();
    std::fs::write(&path, vec![b' '; MAX_ENVELOPE_BYTES + 1]).unwrap();
    assert!(matches!(
        prepare(root.as_fd(), Cause { uid: 0 }),
        Err(LifecycleRefusal::PlanUpdateRefused {
            case: PlanUpdateCase::MalformedPlan,
            ..
        })
    ));
}

fn observation(exchange: &ExchangeId, state: &str, wait: u64) -> Value {
    let enter: Value = serde_json::from_slice(include_bytes!(
        "../../weaver-types/tests/fixtures/state-enter.json"
    ))
    .unwrap();
    let p = &enter["payload"]["body"]["payload"];
    let b = &p["binding"];
    let mut v = json!({"exchange":exchange,"position":"close","payload":{"kind":"answer","body":{"kind":"state","state":state,"load":{
        "session":p["session"],"run":p["run"],"declaration":p["declaration"],"artifact":"/synthetic/decoder.gguf",
        "residual_readout":false,"surprisal":false,"state":{"serving":{"state_management":b["state_management"],"start":b["start"],
        "ste100_scheme":b["ste100_scheme"],"ste100_binary_sha256":"e".repeat(64)}},
        "state_store":{"engine":"sqlite"},"state_member":true,"composer":{"binary":"worker"}
    }}}});
    v["payload"]["body"]["load"]["state"]["serving"]["state_management"]["ste100"]["decision_wait"]
        ["elapsed_ns"] = json!(wait.to_string());
    v
}

fn receive(peer: &std::os::fd::OwnedFd) -> OrganEnvelope {
    let mut bytes = vec![0; MAX_ENVELOPE_BYTES];
    let count = recv(peer.as_raw_fd(), &mut bytes, MsgFlags::empty()).unwrap();
    assert!(count > 0);
    decode_strict(std::str::from_utf8(&bytes[..count]).unwrap()).unwrap()
}
fn transmit(peer: &std::os::fd::OwnedFd, value: &Value) {
    let bytes = serde_json::to_vec(value).unwrap();
    assert_eq!(
        send(peer.as_raw_fd(), &bytes, MsgFlags::MSG_NOSIGNAL).unwrap(),
        bytes.len()
    );
}

#[test]
fn root_plan_exchange_preserves_receipts_refusals_and_lost_answer_without_replay() {
    for case in [
        "updated",
        "stale",
        "read-refused",
        "eof",
        "timeout",
        "foreign-exchange",
        "wrong-position",
        "wrong-seed",
        "active",
    ] {
        let (end, peer) = socketpair(
            AddressFamily::Unix,
            SockType::SeqPacket,
            None,
            SockFlag::SOCK_CLOEXEC,
        )
        .unwrap();
        let (update, update_peer) = socketpair(
            AddressFamily::Unix,
            SockType::SeqPacket,
            None,
            SockFlag::SOCK_CLOEXEC,
        )
        .unwrap();
        let worker = std::thread::spawn(move || {
            let observed = receive(&peer);
            assert_eq!(
                observed.payload,
                Payload::Directive(LifecycleDirective::Observe)
            );
            let facts = observation(
                &observed.exchange,
                if case == "active" { "active" } else { "idle" },
                if case == "timeout" {
                    10_000_000
                } else {
                    2_000_000_000
                },
            );
            transmit(&peer, &facts);
            // The actual worker ends Observe's connection before the next
            // management operation. Reusing it cannot deliver the update.
            drop(peer);
            if case == "active" {
                return;
            }
            let peer = update_peer;
            let request = receive(&peer);
            assert_eq!(request.exchange.ordinal, 1);
            assert!(matches!(
                request.payload,
                Payload::Directive(LifecycleDirective::PlanUpdate { .. })
            ));
            if case == "eof" {
                return;
            }
            if case == "timeout" {
                let mut bytes = [0; 1];
                assert_eq!(
                    recv(peer.as_raw_fd(), &mut bytes, MsgFlags::empty()).unwrap(),
                    0
                );
                return;
            }
            let load = &facts["payload"]["body"]["load"];
            let mut answer = json!({"exchange":request.exchange,"position":"close","payload":{"kind":"answer","body":{"kind":"plan_updated",
                "plan":{"plan_id":{"seed_id":load["state"]["serving"]["start"]["seed_id"],"local_id":"1"},"revision":"1"},
                "source":{"block_id":{"run":load["run"],"sequence":"28","block":"0"},"turn":null}}}});
            if matches!(case, "stale" | "read-refused") {
                answer["payload"] = json!({"kind":"refusal","body":{"kind":"plan_update_refused","case":if case=="stale" {"stale_plan"} else {"read_refused"},
                    "detail":"exact worker detail","rejected_blocks":[],"read_code":if case=="stale" {Value::Null} else {json!("result_unavailable")}}});
            }
            if case == "foreign-exchange" {
                answer["exchange"]["ordinal"] = json!(99)
            }
            if case == "wrong-position" {
                answer["position"] = json!("open")
            }
            if case == "wrong-seed" {
                answer["payload"]["body"]["plan"]["plan_id"]["seed_id"] = json!("foreign")
            }
            transmit(&peer, &answer);
            let mut bytes = [0; 1];
            assert_eq!(
                recv(peer.as_raw_fd(), &mut bytes, MsgFlags::empty()).unwrap(),
                0
            );
        });
        let mut channels = [end, update].into_iter();
        let mut dials = 0;
        let result = exchange(
            prepare_bytes(document("OPEN the file."), Cause { uid: 0 }).unwrap(),
            || {
                dials += 1;
                Ok(Coordination::adopt(
                    channels.next().expect("no third dial or replay"),
                ))
            },
        );
        assert_eq!(dials, if case == "active" { 1 } else { 2 });
        match (case, result) {
            ("updated", Ok(LifecycleAnswer::PlanUpdated { .. })) => (),
            (
                "stale",
                Err(LifecycleRefusal::PlanUpdateRefused {
                    case: PlanUpdateCase::StalePlan,
                    detail,
                    read_code: None,
                    ..
                }),
            ) if detail == "exact worker detail" => (),
            (
                "read-refused",
                Err(LifecycleRefusal::PlanUpdateRefused {
                    case: PlanUpdateCase::ReadRefused,
                    detail,
                    read_code: Some(weaver_types::state_flow::ReadRefusalCode::ResultUnavailable),
                    ..
                }),
            ) if detail == "exact worker detail" => (),
            ("eof" | "timeout", Err(LifecycleRefusal::Unanswered)) => (),
            (
                "foreign-exchange" | "wrong-position" | "wrong-seed",
                Err(LifecycleRefusal::Malformed),
            ) => (),
            ("active", Err(LifecycleRefusal::ActivityNotAtRest)) => (),
            other => panic!("{other:?}"),
        }
        worker.join().unwrap();
    }
}

#[test]
fn root_plan_send_and_read_share_the_original_deadline() {
    let (end, peer) = socketpair(
        AddressFamily::Unix,
        SockType::SeqPacket,
        None,
        SockFlag::SOCK_CLOEXEC,
    )
    .unwrap();
    let channel = Coordination::adopt(end);
    let expired = Instant::now();
    assert_eq!(
        channel.send_plan_until(b"unused", expired),
        Err(ChannelFault::Unanswered)
    );
    let mut bytes = [0; 1];
    assert_eq!(
        recv(peer.as_raw_fd(), &mut bytes, MsgFlags::MSG_DONTWAIT),
        Err(nix::errno::Errno::EAGAIN)
    );
    assert_eq!(
        channel.recv_plan_until(expired),
        Err(ChannelFault::Unanswered)
    );
}

/// Launched by the harness's actual root/worker/SQLite circuit. Only account
/// lookup and fixture paths differ: authorization, locks, fixed-file reading,
/// UID/pidfd/run-lock authentication and both protocol connections are real.
#[test]
fn root_plan_actual_admin_process() {
    let Some(root) = std::env::var_os("WEAVER_ROOT_PLAN_CIRCUIT") else {
        return;
    };
    assert!(nix::unistd::geteuid().is_root());
    let root = std::path::PathBuf::from(root);
    let config = ServiceConfig {
        agent: "plan-fixture".into(),
        coordination_root: root.join("coord"),
        worker: root.join("unused-worker"),
        spu: root.join("unused-spu"),
        gate: root.join("unused-gate"),
        headroom_bytes: None,
        library_path: None,
        shell_binary: None,
        load_bound: Duration::from_secs(10),
        territory: root.clone(),
        territory_fd: None,
        save_points: None,
        territory_unjudged: false,
        operator: 0,
        access_gid: 0,
        boundary: Err("fixture: no role/artifact qualification".into()),
        root: root.join("admin"),
    };
    let dial = || crate::channel::dial(&root.join("admin.sock"), 0, &config.run_directory());
    let request =
        crate::surface::parse_foundation_arguments(["plan-update".into(), config.agent.clone()])
            .unwrap();
    let answer = crate::dispatch_with_plan(&config, request, |config, agent| {
        update_with(config, agent, dial)
    })
    .unwrap();
    let lines = std::fs::read_to_string(config.admin_log()).unwrap();
    assert_eq!(lines.lines().count(), 1, "one dispatch log, no replay");
    let line: Value = serde_json::from_str(lines.trim()).unwrap();
    assert_eq!(line["verb"], "plan-update");
    assert_eq!(line["command"], "weaver-admin plan-update plan-fixture");
    assert_eq!(line["uid"], 0);
    assert_eq!(
        serde_json::from_str::<LifecycleAnswer>(line["outcome"].as_str().unwrap()).unwrap(),
        answer
    );
    let mut channel = dial().unwrap();
    let observed = crate::serving_observe::ask(&mut channel).unwrap();
    let ObservationAnswer::State {
        plan: Some(summary),
        ..
    } = &observed
    else {
        panic!("committed Observe required: {observed:?}");
    };
    assert_eq!(
        answer,
        LifecycleAnswer::PlanUpdated {
            plan: summary.plan.clone(),
            source: summary.last_source.clone()
        }
    );
    let document = std::fs::read(config.root.join("plan-update.json")).unwrap();
    assert_eq!(
        summary.document_sha256.as_str(),
        format!("{:x}", Sha256::digest(document))
    );
    std::fs::write(
        root.join("admin-plan-answer.json"),
        serde_json::to_vec(&answer).unwrap(),
    )
    .unwrap();
}

#[test]
fn root_plan_command_admission_child() {
    let Ok(args) = std::env::var("WEAVER_ROOT_PLAN_COMMAND_ARGS") else {
        return;
    };
    let args: Vec<String> = serde_json::from_str(&args).unwrap();
    let result = crate::run_foundation_arguments(args);
    crate::finish_command(result)
}

#[test]
#[ignore = "actual root command admission with nonroot sudo cause"]
fn root_plan_command_refuses_before_config_and_never_waits_for_stdin() {
    assert!(nix::unistd::geteuid().is_root());
    let root = crate::scratch::Scratch(std::env::temp_dir().join(format!(
        "root-plan-command-{}",
        crate::random::random_hex(12).unwrap()
    )));
    for (cause, args, expected) in [
        (
            "73",
            vec!["plan-update", "alpha"],
            LifecycleRefusal::Unauthorized,
        ),
        (
            "+0",
            vec!["plan-update", "alpha"],
            LifecycleRefusal::Malformed,
        ),
        (
            "0",
            vec!["plan-update", "alpha", "--stdin"],
            LifecycleRefusal::Malformed,
        ),
    ] {
        let mut child = std::process::Command::new(std::env::current_exe().unwrap())
            .args([
                "--exact",
                "serving_plan::tests::root_plan_command_admission_child",
                "--nocapture",
            ])
            .env(
                "WEAVER_ROOT_PLAN_COMMAND_ARGS",
                serde_json::to_string(&args).unwrap(),
            )
            .env("SUDO_UID", cause)
            .env("WEAVER_ADMIN_CONFIG", root.as_ref())
            .stdin(std::process::Stdio::piped())
            .stdout(std::process::Stdio::piped())
            .stderr(std::process::Stdio::piped())
            .spawn()
            .unwrap();
        // Retain an empty, open stdin pipe. Any stdin read would block instead
        // of completing this fixed command's authorization/shape refusal.
        let input = child.stdin.take().unwrap();
        let end = Instant::now() + Duration::from_secs(5);
        loop {
            if child.try_wait().unwrap().is_some() {
                break;
            }
            if Instant::now() >= end {
                child.kill().unwrap();
                let _ = child.wait();
                panic!("plan command waited for stdin or ignored its admission refusal");
            }
            std::thread::sleep(Duration::from_millis(1));
        }
        let output = child.wait_with_output().unwrap();
        drop(input);
        assert_eq!(output.status.code(), Some(crate::surface::EXIT_REFUSED));
        let stdout = String::from_utf8(output.stdout).unwrap();
        let records: Vec<_> = stdout
            .lines()
            .filter(|line| line.starts_with('{'))
            .collect();
        assert_eq!(
            records.len(),
            1,
            "one command answer beside test-runner output"
        );
        assert_eq!(
            serde_json::from_str::<LifecycleRefusal>(records[0]).unwrap(),
            expected
        );
        assert!(!root.as_ref().exists());
    }
}
