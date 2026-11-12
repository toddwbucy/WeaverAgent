//! **The record identity of the sampling carrier**, per `weaver-spu-Spec`
//! section 8: an instrument, run by name, that drives the SPU binary through
//! one admit, one open and one generation on a device lane, and writes what
//! the record takes from that generation's answers. A run on the tree before
//! the carrier and a run on the tree after it, the declaration's defaults at
//! the old frozen constants, then compare byte for byte.
//!
//! **What it writes is what the harness splices without reading**: every
//! token frame as it crossed, and the closing answer's request box,
//! measurement, emission, finish and content, each as the bytes that crossed.
//! The harness's own events change only on a refused append, which this run
//! does not reach.
//!
//! **The file speaks the wire as raw JSON**, so it copies unchanged into the
//! tree before the carrier, untracked for the run and removed after, as
//! `tests/engine_settings_identity.rs` copies. `WEAVER_IDENTITY_SHAPE=admit`
//! is that tree's shape: the seed rides the admit, and the append carries no
//! sampling and no bound. `carried` is this tree's: the admit carries no
//! sampling name, and the append carries the six values and the bound. Both
//! admit the same ceiling and the append's bound is that ceiling, so the
//! two shapes ask for one generation.
//!
//! **Both runs load one build of the engine libraries.** The measurement's
//! engine block names each library the process loaded and its digest, and two
//! builds of one engine source in two targets differed in four of five
//! digests when this instrument first ran (2026-11-12), so two runs on their
//! own builds differ there whatever the carrier does. `cargo test` puts its
//! own target's library directory first on `LD_LIBRARY_PATH`, so the tree
//! after the carrier runs its built test binary directly, with
//! `LD_LIBRARY_PATH` naming the library directory of the tree before's build.
//!
//! Run the tree before with the lane's mask, then the tree after on that
//! build's libraries, then compare:
//!
//! ```text
//! CUDA_DEVICE_ORDER=PCI_BUS_ID CUDA_VISIBLE_DEVICES=<ordinals> \
//! WEAVER_IDENTITY_ARTIFACT=<gguf> WEAVER_IDENTITY_OUT=<json> \
//! WEAVER_IDENTITY_SHAPE=admit \
//! cargo test -p weaver-spu --features cuda,gguf --locked \
//!     --test request_identity -- --ignored --exact record_one_turn_through_the_binary
//!
//! LD_LIBRARY_PATH=<the tree before's llama-cpp-sys-2 out/lib> \
//! CUDA_DEVICE_ORDER=PCI_BUS_ID CUDA_VISIBLE_DEVICES=<ordinals> \
//! WEAVER_IDENTITY_ARTIFACT=<gguf> WEAVER_IDENTITY_OUT=<json> \
//! WEAVER_IDENTITY_SHAPE=carried \
//! <the tree after's request_identity test binary> \
//!     --ignored --exact record_one_turn_through_the_binary
//!
//! WEAVER_IDENTITY_BEFORE=<json> WEAVER_IDENTITY_AFTER=<json> \
//! cargo test -p weaver-spu --features cuda,gguf --locked \
//!     --test request_identity -- --ignored --exact the_two_records_agree
//! ```
//!
//! The binding names the mask's ordinals, which is the lane: one ordinal is
//! the one-card lane and two are the layer split.

#![cfg(all(feature = "gguf", feature = "cuda"))]

mod common;

use std::collections::HashMap;
use std::os::fd::AsRawFd;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

use serde_json::value::RawValue;
use weaver_types::{
    ArtifactRef, DecoderInstruction, DeviceOrdinal, LifecycleAnswer, LifecycleDirective,
    ModelBinding, Payload, SpuInstruction,
};

/// The constants every serving declaration carried frozen before the carrier.
const TEMPERATURE: f64 = 0.7;
const TOP_K: u32 = 40;
const TOP_P: f64 = 0.95;
const REPETITION_PENALTY: f64 = 1.1;
const REPETITION_WINDOW: u32 = 64;
const SEED: u64 = 4242;
/// The session's ceiling, and the bound the carried shape sends: longer than
/// the penalty window, so the window wraps inside the run.
const CEILING: u64 = 160;
const PROMPT: &str = "Explain, in a short paragraph, why the sky looks blue.";

/// A JSON object read member by member, each member's bytes as they crossed.
type Members = HashMap<String, Box<RawValue>>;

fn named(variable: &str) -> String {
    std::env::var(variable).unwrap_or_else(|_| panic!("{variable} names this run's input"))
}

/// The commit the tree stood at and its `git status` lines, so a record
/// names the build that made it.
fn tree() -> serde_json::Value {
    let root = Path::new(env!("CARGO_MANIFEST_DIR"));
    let git = |args: &[&str]| -> String {
        let out = Command::new("git")
            .arg("-C")
            .arg(root)
            .args(args)
            .output()
            .expect("git runs");
        assert!(out.status.success(), "git {args:?} answers");
        String::from_utf8(out.stdout).expect("git answers in UTF-8")
    };
    serde_json::json!({
        "head": git(&["rev-parse", "HEAD"]).trim(),
        "status": git(&["status", "--porcelain"]).lines().collect::<Vec<_>>(),
    })
}

fn member<'a>(object: &'a Members, name: &str) -> &'a str {
    object
        .get(name)
        .unwrap_or_else(|| panic!("the object carries {name}"))
        .get()
}

#[test]
#[ignore = "an instrument, run by name under a lane's mask"]
fn record_one_turn_through_the_binary() {
    let artifact = PathBuf::from(named("WEAVER_IDENTITY_ARTIFACT"));
    assert!(
        artifact.is_file(),
        "WEAVER_IDENTITY_ARTIFACT names {}, which is not a regular file",
        artifact.display()
    );
    let out = PathBuf::from(named("WEAVER_IDENTITY_OUT"));
    let shape = named("WEAVER_IDENTITY_SHAPE");
    let carried = match shape.as_str() {
        "carried" => true,
        "admit" => false,
        other => panic!("WEAVER_IDENTITY_SHAPE is admit or carried, not {other}"),
    };
    let ordinals: Vec<u32> = named("CUDA_VISIBLE_DEVICES")
        .split(',')
        .map(|ordinal| ordinal.trim().parse().expect("a decimal ordinal"))
        .collect();
    if let Err(why) = common::lease(&ordinals) {
        panic!("the lane is not leased: {why}");
    }

    let (lifecycle, child_lifecycle) = common::seqpacket_pair();
    let (decode, child_decode) = common::seqpacket_pair();
    let log_path = out.with_extension("spu.log");
    let log = std::fs::File::create(&log_path).expect("the child's log opens");
    // The mask is this process's, which the child inherits as the harness's
    // fork composes it (PAIR-1).
    let mut command = Command::new(env!("CARGO_BIN_EXE_weaver-spu"));
    command
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::from(log));
    common::place_inherited(
        &mut command,
        &[child_lifecycle.as_raw_fd(), child_decode.as_raw_fd()],
    );
    let mut child = command.spawn().expect("the binary starts");
    drop((child_lifecycle, child_decode));
    common::bound_receives(&lifecycle, 300);
    common::bound_receives(&decode, 300);
    let decode = weaver_spu::channel::decode_from_owned(decode);

    let mut tunable_values: std::collections::BTreeMap<String, f64> = [
        ("context-capacity".to_string(), 4096.0),
        ("max-tokens-per-turn".to_string(), CEILING as f64),
    ]
    .into_iter()
    .collect();
    if !carried {
        tunable_values.insert("seed".to_string(), SEED as f64);
    }
    let admitted = common::ask(
        &lifecycle,
        1,
        LifecycleDirective::Admit {
            instruction: SpuInstruction {
                classify: None,
                decoder: DecoderInstruction {
                    model_binding: ModelBinding {
                        artifact: ArtifactRef(artifact.to_string_lossy().into_owned()),
                        devices: ordinals.iter().map(|o| DeviceOrdinal(*o)).collect(),
                        adapter: None,
                    },
                    residual_readout_election: false,
                    field_election: None,
                    // Surprisal elected, so the measurement carries both
                    // signals and the comparison reads more than the tokens.
                    surprisal_election: true,
                    refeed_permission: false,
                    column_permission: false,
                    tunable_values,
                },
            },
        },
    );
    assert_eq!(
        admitted.payload,
        Payload::Answer(LifecycleAnswer::Admitted),
        "the artifact admits"
    );

    let send = |directive: serde_json::Value| {
        let body = serde_json::to_vec(&directive).expect("a directive renders");
        decode.send_octets(&body).expect("the frame sends");
    };
    let recv = || -> (String, Members, String) {
        let frame = decode.recv_octets().expect("an answer arrives");
        let text = String::from_utf8(frame).expect("the answer is UTF-8");
        let members: Members = serde_json::from_str(&text).expect("the answer is an object");
        let kind: String =
            serde_json::from_str(member(&members, "kind")).expect("the answer names its kind");
        (kind, members, text)
    };
    let text = |role: &str, text: &str| {
        serde_json::json!({
            "role": role,
            "content": [{"type": "text", "text": text}],
        })
    };

    send(serde_json::json!({
        "kind": "open",
        "session": "s-identity",
        "messages": [text("system", "You answer plainly.")],
        "column_ask": false,
    }));
    let (kind, _, opened) = recv();
    assert_eq!(kind, "opened", "the session opens: {opened}");

    let mut append = serde_json::json!({
        "kind": "append_and_generate",
        "turn": "t-1",
        "delta": [text("user", PROMPT)],
        "tool_schemas": [],
    });
    if carried {
        append["sampling"] = serde_json::json!({
            "temperature": TEMPERATURE,
            "top_k": TOP_K,
            "top_p": TOP_P,
            "repetition_penalty": REPETITION_PENALTY,
            "repetition_window": REPETITION_WINDOW,
            "seed": SEED,
        });
        append["max_tokens"] = serde_json::json!(CEILING);
    }
    send(append);
    let mut frames = Vec::new();
    let body = loop {
        let (kind, members, text) = recv();
        match kind.as_str() {
            "token" => frames.push(text),
            "generated" => {
                break serde_json::from_str::<Members>(member(&members, "body"))
                    .expect("the close's body is an object");
            }
            other => panic!("tokens then the close, got {other}: {text}"),
        }
    };
    let record = serde_json::json!({
        "tree": tree(),
        "shape": &shape,
        "artifact": artifact.to_string_lossy(),
        "devices": &ordinals,
        "token_frames": &frames,
        "request": member(&body, "request"),
        "measurement": member(&body, "measurement"),
        "emission": member(&body, "emission"),
        "finish": member(&body, "finish"),
        "content": member(&body, "content"),
    });
    std::fs::write(&out, format!("{record}\n")).expect("the record writes");

    drop(decode);
    drop(lifecycle);
    common::wait_bounded(&mut child, 120, "the SPU's exit after its seams closed");
    eprintln!(
        "recorded {} token frames on {:?} under the {shape} shape to {}",
        frames.len(),
        ordinals,
        out.display()
    );
}

/// The two records agree byte for byte, the measurement's `timings` aside,
/// which differ between any two runs (`weaver-spu-Spec` section 8).
#[test]
#[ignore = "an instrument, run by name over two records"]
fn the_two_records_agree() {
    let read = |variable: &str| -> serde_json::Value {
        let path = named(variable);
        let text = std::fs::read_to_string(&path).unwrap_or_else(|_| panic!("{path} reads"));
        serde_json::from_str(&text).expect("a record is JSON")
    };
    let before = read("WEAVER_IDENTITY_BEFORE");
    let after = read("WEAVER_IDENTITY_AFTER");
    assert_eq!(
        before["shape"], "admit",
        "the before record is the admit shape"
    );
    assert_eq!(
        after["shape"], "carried",
        "the after record is the carried shape"
    );
    for key in ["artifact", "devices"] {
        assert_eq!(before[key], after[key], "the two runs share the {key}");
    }
    for key in ["token_frames", "request", "emission", "finish", "content"] {
        assert_eq!(before[key], after[key], "the {key} bytes agree");
    }
    let measurement = |record: &serde_json::Value| -> Vec<(String, String)> {
        let members: Members = serde_json::from_str(
            record["measurement"]
                .as_str()
                .expect("the measurement's bytes"),
        )
        .expect("the measurement is an object");
        let mut kept: Vec<(String, String)> = members
            .into_iter()
            .filter(|(name, _)| name != "timings")
            .map(|(name, value)| (name, value.get().to_string()))
            .collect();
        kept.sort();
        kept
    };
    let (before_measurement, after_measurement) = (measurement(&before), measurement(&after));
    for name in ["output_tokens", "entropies", "surprisals"] {
        assert!(
            before_measurement.iter().any(|(held, _)| held == name),
            "the measurement carries its {name}, so the comparison reads more than the tokens"
        );
    }
    assert_eq!(
        before_measurement, after_measurement,
        "the measurement's bytes agree, its timings aside"
    );
    let frames = before["token_frames"].as_array().map_or(0, Vec::len);
    eprintln!(
        "the records agree: {frames} token frames, the request box, the measurement less its \
         timings, the emission, the finish and the content, on {:?}",
        before["devices"]
    );
}
