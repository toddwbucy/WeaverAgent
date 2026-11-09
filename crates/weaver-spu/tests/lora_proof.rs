//! **The real 30B application proof of LORA-7, on one pinned input.**
//!
//! `weaver-spu-lora-loading-Spec-amendment` section 7: a base-only control,
//! repeated, and an adapted run, repeated, on the fixed base, fixture, input
//! and settings, under the backend's layer split on the binding's devices. The
//! adapted run must change at least one output token, each pair must replay
//! bitwise, and all four arms must show the same division. The adapted pair
//! repeats because the adapted model is the one served (the operator's ruling
//! of 2026-11-08, the adapter is the point).
//!
//! **Bitwise numerical replay, not token identity alone** (the operator's
//! ruling of 2026-11-08): a pair must carry identical token ids,
//! bit-identical entropy and surprisal at every position (the f32 values
//! `weaver-spu-Spec` section 6 computes before the sampler, see
//! `fixture::CALCULATION`), and the same termination. A pair that matches its
//! ids and differs in one bit fails, naming the position and both values:
//! that is a finding for the operator's drift ruling, not a test defect. Each
//! arm runs in its own process (see `lora/fixture.rs`).
//!
//! An explicit accelerator instrument, run with `--ignored` under the
//! variables `lora/fixture.rs` reads and `WEAVER_LORA_PINNED_ID`, the entry to
//! decode. A missing input fails by name.
//!
//! Perturbation: run the adapted arm without its request (`adapted` false at
//! the third `run_arm`) and the output-change assertion fails, because the
//! third arm decodes the control again.
#![cfg(all(feature = "gguf", feature = "cuda"))]

mod common;
#[path = "lora/fixture.rs"]
mod fixture;

use fixture::{
    Inputs, Trace, assert_covers, assert_same_division, first_difference,
    first_numerical_difference, run_arm,
};
use weaver_spu::division::Division;
use weaver_spu::lora::observation::Record;
use weaver_spu::room::Caches;
use weaver_types::DeviceOrdinal;

const TEST: &str = "the_adapter_changes_the_pinned_greedy_output_on_the_binding";

/// **One path**, per the adapter binding amendment ADP-2 and ADP-6: the
/// request serving builds from the declared table is LORA-1's request for
/// the same inputs. The fixture declares that table in its binding and
/// admits through `Residency::admit`, so the LORA-7 proof runs the
/// production entry itself. Needs no device.
///
/// Perturbation: build the serving request as a second type, or drop or
/// recompute a member in `Request::from`, and this fails.
#[test]
fn the_declared_request_is_the_fixtures_request() {
    let inputs = Inputs {
        base: "/models/base.gguf".into(),
        adapter: "/models/adapter.gguf".into(),
        adapter_sha256: "a".repeat(64),
        base_blake3: "b".repeat(64),
        scale: 0.7,
        entries: "/models/entries.jsonl".into(),
        report: "/reports/arm".into(),
        devices: vec![DeviceOrdinal(0), DeviceOrdinal(1)],
    };
    assert_eq!(
        weaver_spu::lora::Request::from(&inputs.declared()),
        inputs.request()
    );
}

#[test]
#[ignore = "needs the base, the adapter, the entries and the binding's devices"]
fn the_adapter_changes_the_pinned_greedy_output_on_the_binding() {
    if fixture::child() {
        return;
    }
    let inputs = Inputs::from_env();
    let pinned = std::env::var("WEAVER_LORA_PINNED_ID")
        .expect("WEAVER_LORA_PINNED_ID is not set, so the fixture did not run");
    if let Err(why) = common::lease(&inputs.ordinals()) {
        panic!("the binding's devices are not leased, so the fixture did not run: {why}");
    }
    fixture::fresh_report(&inputs.report);
    let ids = vec![pinned.clone()];

    let control = run_arm(TEST, &inputs, "control", false, Some(&ids));
    let repeat = run_arm(TEST, &inputs, "control-repeat", false, Some(&ids));
    let adapted = run_arm(TEST, &inputs, "adapted", true, Some(&ids));
    let adapted_repeat = run_arm(TEST, &inputs, "adapted-repeat", true, Some(&ids));

    for arm in [&control, &repeat, &adapted, &adapted_repeat] {
        assert_covers(arm, &ids);
        assert_same_division(&control, arm);
    }

    // The summary is written before the assertions, so a pair that does not
    // repeat leaves its first difference in the report.
    let base = &control.outputs[&pinned];
    let tuned = &adapted.outputs[&pinned];
    let control_departs =
        first_numerical_difference(&control.traces[&pinned], &repeat.traces[&pinned]);
    let adapted_departs =
        first_numerical_difference(&adapted.traces[&pinned], &adapted_repeat.traces[&pinned]);
    let changed = first_difference(base, tuned);
    let summary = serde_json::json!({
        "pinned": pinned,
        "control_tokens": base.len(),
        "adapted_tokens": tuned.len(),
        "first_changed_position": changed,
        "replay": "bitwise: token ids, entropy and surprisal f32 bit patterns at every position, and termination",
        "control_replayed_bitwise": control_departs.is_none(),
        "control_first_difference": control_departs,
        "adapted_replayed_bitwise": adapted_departs.is_none(),
        "adapted_first_difference": adapted_departs,
        "steps_compared": {
            "control": control.traces[&pinned].tokens.len(),
            "adapted": adapted.traces[&pinned].tokens.len(),
        },
        "same_division": true,
    });
    std::fs::write(
        inputs.report.join("summary.json"),
        serde_json::to_string_pretty(&summary).expect("the summary serializes"),
    )
    .expect("the summary writes");
    if let Some(difference) = &control_departs {
        panic!("the base-only control does not replay bitwise on {pinned}: {difference}");
    }
    if let Some(difference) = &adapted_departs {
        panic!("the adapted run does not replay bitwise on {pinned}: {difference}");
    }
    assert!(
        changed.is_some(),
        "the adapted run changed no output token on {pinned}"
    );
    eprintln!("LORA-7 proof: {summary}");
}

fn records(lines: &[&str]) -> Vec<Record> {
    lines
        .iter()
        .map(|line| Record {
            level: llama_cpp_sys_2::GGML_LOG_LEVEL_DEBUG as i32,
            bytes: format!("{line}\n").into_bytes(),
        })
        .collect()
}

/// The shapes the pinned engine writes for a two-device layer split of a
/// model with two repeating layers, opened in two sessions. The assignment
/// records come from the load, once: layers 0 and 1 and the output layer 2.
/// Each session's context writes the KV records of layers 0 and 1 again.
const TWO_SESSIONS: &[&str] = &[
    "load_tensors: layer   0 assigned to device CUDA0, is_swa = 0",
    "load_tensors: layer   1 assigned to device CUDA1, is_swa = 0",
    "load_tensors: layer   2 assigned to device CUDA1, is_swa = 0",
    "llama_kv_cache: layer   0: dev = CUDA0",
    "llama_kv_cache: layer   1: dev = CUDA1",
    "llama_kv_cache: layer   0: dev = CUDA0",
    "llama_kv_cache: layer   1: dev = CUDA1",
];
/// Where the load's records end, and each session's.
const LOAD: usize = 3;
const FIRST: std::ops::Range<usize> = 3..5;
const SECOND: std::ops::Range<usize> = 5..7;

fn session(records: &[Record], range: std::ops::Range<usize>) -> Result<Vec<u32>, String> {
    let devices = [DeviceOrdinal(0), DeviceOrdinal(1)];
    let division = Division::even(2, 2).expect("two layers over two devices");
    let caches = [Caches {
        attention: true,
        recurrent: false,
    }; 2];
    fixture::judge_session(records, LOAD, range, &devices, &division, &caches)
        .map(|observed| observed.slots.iter().map(|d| d.0).collect())
}

/// **Each session is judged with the load's records and its own**, so the
/// SPU's judgment passes on every session of a capture of several.
#[test]
fn each_session_judges_with_the_load_records() {
    let records = records(TWO_SESSIONS);
    assert_eq!(session(&records, FIRST), Ok(vec![0, 1, 1]));
    assert_eq!(session(&records, SECOND), Ok(vec![0, 1, 1]));
}

/// Perturbation of the above: judged whole, the second session's records are
/// repeats, and the judgment refuses them.
#[test]
fn a_capture_judged_whole_refuses_its_repeats() {
    let records = records(TWO_SESSIONS);
    let faults = session(&records, FIRST.start..SECOND.end).expect_err("repeats refuse");
    assert!(faults.contains("recorded 2 times"), "{faults}");
}

/// A later session's cache on the wrong device refuses, which a judgment of
/// the first session alone would not see.
#[test]
fn a_later_session_off_its_layer_refuses() {
    let mut lines = TWO_SESSIONS.to_vec();
    lines[6] = "llama_kv_cache: layer   1: dev = CUDA0";
    let records = records(&lines);
    assert!(session(&records, FIRST).is_ok());
    let faults = session(&records, SECOND).expect_err("the misplaced cache refuses");
    assert!(faults.contains("layer 1"), "{faults}");
}

/// The boundaries are counts, not a reading of the log: a session whose cache
/// also prints a filtered layer 0 line, as a sliding-window model's second
/// cache does, is judged whole.
#[test]
fn a_filtered_cache_line_does_not_move_the_boundaries() {
    let mut lines = TWO_SESSIONS[..5].to_vec();
    lines.insert(4, "llama_kv_cache: layer   0: filtered");
    let records = records(&lines);
    assert_eq!(session(&records, 3..6), Ok(vec![0, 1, 1]));
}

fn trace() -> Trace {
    Trace {
        tokens: vec![10, 11, 12],
        entropy: vec![1.5f32.to_bits(), 0.25f32.to_bits(), 3.0f32.to_bits()],
        surprisal: vec![0.5f32.to_bits(), 0.125f32.to_bits(), 2.0f32.to_bits()],
        finish: "stop".into(),
        stop: Some(100257),
    }
}

/// **Two bitwise-identical traces replay**, and a trace survives its own
/// report line.
#[test]
fn identical_traces_replay_bitwise() {
    let a = trace();
    let b = Trace::from_json(&a.to_json("t001"));
    assert_eq!(a, b);
    assert_eq!(first_numerical_difference(&a, &b), None);
}

/// Perturbation of the above: one bit of one recorded value flipped, with the
/// ids unchanged, fails naming the position and both values.
#[test]
fn one_flipped_bit_fails_naming_its_position() {
    let a = trace();
    let mut b = trace();
    b.surprisal[1] ^= 1;
    let difference = first_numerical_difference(&a, &b).expect("one bit differs");
    assert!(
        difference.starts_with("position 1: surprisal_bits 3e000000"),
        "{difference}"
    );
    assert!(difference.contains("3e000001"), "{difference}");
    let mut c = trace();
    c.entropy[2] ^= 1 << 31;
    let difference = first_numerical_difference(&a, &c).expect("the sign bit differs");
    assert!(
        difference.starts_with("position 2: entropy_bits"),
        "{difference}"
    );
}

/// A differing token is named before its values.
#[test]
fn a_differing_token_is_named_at_its_position() {
    let a = trace();
    let mut b = trace();
    b.tokens[0] = 99;
    assert_eq!(
        first_numerical_difference(&a, &b).as_deref(),
        Some("position 0: token Some(10) against Some(99)")
    );
}

/// The same steps with another termination do not replay.
#[test]
fn a_differing_termination_does_not_replay() {
    let a = trace();
    let mut b = trace();
    b.finish = "length".into();
    b.stop = None;
    let difference = first_numerical_difference(&a, &b).expect("the termination differs");
    assert!(difference.starts_with("termination:"), "{difference}");
}

/// A trace one step longer does not replay, and the account names the step.
#[test]
fn a_longer_trace_does_not_replay() {
    let a = trace();
    let mut b = trace();
    b.tokens.push(13);
    b.entropy.push(1.0f32.to_bits());
    b.surprisal.push(1.0f32.to_bits());
    assert_eq!(
        first_numerical_difference(&a, &b).as_deref(),
        Some("position 3: token None against Some(13)")
    );
}

/// **A failed arm keeps the entries it completed** (LORA-6, Codex round 1 on
/// #492): two entries appended, then a failure on the third, leave both on
/// disk in the outputs and the trace, and the failure record's count names the
/// second as the last.
#[test]
fn a_failed_arm_keeps_its_completed_entries() {
    let report = std::env::temp_dir().join(format!(
        "lora-entry-log-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .expect("the clock is past the epoch")
            .as_nanos()
    ));
    std::fs::create_dir(&report).expect("a scratch report directory");
    let path = report.clone();
    let failed = std::panic::catch_unwind(move || {
        let mut log = fixture::EntryLog::create(&path, "arm");
        for id in ["t001", "t002"] {
            log.append(
                &serde_json::json!({"id": id, "token_ids": [1, 2]}),
                &trace().to_json(id),
            );
        }
        panic!("entry t003 failed");
    });
    assert!(failed.is_err());
    let lines = |file: &str| -> Vec<String> {
        std::fs::read_to_string(report.join(file))
            .expect("the file is on disk")
            .lines()
            .map(str::to_string)
            .collect()
    };
    assert_eq!(lines("arm.jsonl").len(), 2);
    assert_eq!(lines("arm.trace.jsonl").len(), 2);
    assert_eq!(
        fixture::completed_entries(&report, "arm"),
        (2, Some("t002".to_string()))
    );
    std::fs::remove_dir_all(&report).expect("the scratch directory goes");
}

/// **The serving binary admits a declared adapter, and the record carries
/// it**, per the adapter binding amendment ADP-2 and ADP-3 and their ADP-6
/// rows "The serving binary observes", "The engine log keeps flowing" and
/// "The identity is recorded": the SPU binary, spawned as the harness's fork
/// spawns it under the binding's mask, admits a binding whose `adapter` table
/// declares the fixture's adapter. The admit answers `Admitted` rather than
/// refusing at `adapter-observer`, because the binary installed the capture
/// at process start. The Open measurement and the generation's measurement
/// carry the adapter object with the scale written as the f32, and the
/// binary's standard error holds the engine's lines, the adapter's placement
/// lines among them, which the capture forwarded.
///
/// Perturbation: install the capture only in the fixture (drop the install
/// from `main`), and the admit refuses `DeviceCannotAdmit` at
/// `adapter-observer`. Swallow the lines in the capture and the log
/// assertion fails.
#[test]
#[ignore = "needs the base, the adapter and the binding's devices"]
fn the_serving_binary_admits_a_declared_adapter_and_records_it() {
    use std::os::fd::AsRawFd;
    use std::process::Stdio;
    use weaver_traits::{ContentBlock, Message, Role};
    use weaver_types::{
        ArtifactRef, DecoderInstruction, LifecycleAnswer, LifecycleDirective, ModelBinding,
        Payload, SessionId, SpuInstruction, TokenAnswer, TokenDirective, TurnKey,
    };

    let inputs = Inputs::from_env();
    if let Err(why) = common::lease(&inputs.ordinals()) {
        panic!("the binding's devices are not leased, so the instrument did not run: {why}");
    }
    let mask = inputs
        .ordinals()
        .iter()
        .map(u32::to_string)
        .collect::<Vec<_>>()
        .join(",");
    let (lifecycle, child_lifecycle) = common::seqpacket_pair();
    let (decode_parent, child_decode) = common::seqpacket_pair();
    let log_path = std::env::temp_dir().join(format!(
        "weaver-spu-adapter-binding-{}.log",
        std::process::id()
    ));
    let log = std::fs::File::create(&log_path).expect("a child log file");
    let mut command = std::process::Command::new(env!("CARGO_BIN_EXE_weaver-spu"));
    command
        .env("CUDA_DEVICE_ORDER", "PCI_BUS_ID")
        .env("CUDA_VISIBLE_DEVICES", &mask)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::from(log));
    common::place_inherited(
        &mut command,
        &[child_lifecycle.as_raw_fd(), child_decode.as_raw_fd()],
    );
    let mut child = command.spawn().expect("the binary starts");
    common::bound_receives(&lifecycle, 900);
    common::bound_receives(&decode_parent, 900);
    let decode = weaver_spu::channel::decode_from_owned(decode_parent);
    let log_text = || std::fs::read_to_string(&log_path).unwrap_or_default();

    let declared = inputs.declared();
    let admitted = common::ask(
        &lifecycle,
        1,
        LifecycleDirective::Admit {
            instruction: SpuInstruction {
                classify: None,
                decoder: DecoderInstruction {
                    model_binding: ModelBinding {
                        artifact: ArtifactRef(inputs.base.to_string_lossy().into_owned()),
                        devices: inputs.devices.clone(),
                        adapter: Some(Box::new(declared)),
                    },
                    residual_readout_election: false,
                    field_election: None,
                    surprisal_election: false,
                    refeed_permission: false,
                    column_permission: false,
                    tunable_values: [
                        ("max-tokens-per-turn".to_string(), 4.0),
                        ("context-capacity".to_string(), 4096.0),
                        ("seed".to_string(), 11.0),
                    ]
                    .into_iter()
                    .collect(),
                },
            },
        },
    );
    assert_eq!(
        admitted.payload,
        Payload::Answer(LifecycleAnswer::Admitted),
        "the declared adapter admits through the binary: {}",
        log_text()
    );

    let send = |directive: &TokenDirective| {
        let body = serde_json::to_vec(directive).expect("a directive renders");
        decode.send_octets(&body).expect("the frame sends");
    };
    let recv = || -> TokenAnswer {
        let frame = decode.recv_octets().expect("an answer arrives");
        serde_json::from_slice(&frame).expect("the answer parses")
    };
    let text = |role, text: &str| Message {
        role,
        content: vec![ContentBlock::Text { text: text.into() }],
    };
    let scale_text = serde_json::to_string(&inputs.scale).expect("an f32 renders");
    let judge_adapter = |measurement: &str, what: &str| {
        // The instrument's evidence, teed: the measurement's leading members.
        let lead: String = measurement.chars().take(480).collect();
        eprintln!("instrument: {what}: {lead}");
        let parsed: serde_json::Value = serde_json::from_str(measurement).expect("JSON");
        let adapter = &parsed["adapter"];
        assert_eq!(
            adapter["artifact"].as_str(),
            Some(inputs.adapter.to_string_lossy().as_ref()),
            "{what}"
        );
        assert_eq!(
            adapter["sha256"].as_str(),
            Some(inputs.adapter_sha256.as_str()),
            "{what}"
        );
        assert_eq!(
            adapter["scale"].as_f64().map(|s| s as f32),
            Some(inputs.scale),
            "{what}"
        );
        assert!(
            measurement.contains(&format!("\"scale\":{scale_text}}}")),
            "{what} writes the scale as the f32: {measurement}"
        );
    };

    send(&TokenDirective::Open {
        session: SessionId("s-adapter".into()),
        column_ask: false,
        messages: vec![text(Role::System, "You answer plainly.")],
    });
    let TokenAnswer::Opened(opened) = recv() else {
        panic!("the session opens: {}", log_text());
    };
    judge_adapter(opened.measurement.get(), "the Open measurement");

    send(&TokenDirective::AppendAndGenerate {
        tool_schemas: vec![],
        turn: TurnKey("t-1".into()),
        delta: vec![text(Role::User, "Name one colour.")],
    });
    let generation = loop {
        match recv() {
            TokenAnswer::Token { .. } => {}
            TokenAnswer::Generated(generation) => break generation,
            other => panic!("tokens then the close, got {other:?}"),
        }
    };
    judge_adapter(generation.measurement.get(), "the generation's measurement");

    // The decode phase ends when its channel closes, and only then does the
    // binary read the lifecycle seam again, so the release follows the close.
    drop(decode);
    let released = common::ask(&lifecycle, 2, LifecycleDirective::Release);
    assert_eq!(released.payload, Payload::Answer(LifecycleAnswer::Released));
    drop(lifecycle);
    let status = common::wait_bounded(&mut child, 120, "the released worker exits");
    assert!(status.success());

    let log = log_text();
    assert!(
        log.contains("load_tensors:") && log.contains("llama_adapter_lora_init_impl: lora for '"),
        "the engine's lines, kept and not, reach the binary's stderr: {log}"
    );
    eprintln!(
        "instrument: the binary's stderr holds {} lines, the adapter's among them:",
        log.lines().count()
    );
    for line in log
        .lines()
        .filter(|line| line.contains("llama_adapter_lora"))
    {
        eprintln!("instrument:   {line}");
    }
    let _ = std::fs::remove_file(&log_path);
}
