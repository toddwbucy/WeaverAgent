//! conforms: spu-release-frees-before-answering
//! conforms: spu-readout-refused-at-admit
//!
//! The success path: a real artifact admitted onto a real device and released,
//! per `weaver-spu-Spec` section 3 and `weaver-harness-spu-contract` sections 3
//! and 4. This is the path `tests/seam.rs` names as its gap, closed here for
//! the build that carries a backend.
//!
//! Everything in this file needs the `cuda` and `gguf` features, a CUDA device,
//! and a real GGUF on disk. Each absence skips loudly: a suite that reports
//! success for tests it did not run is the failure mode this corpus treats as
//! worse than a red result.

#![cfg(all(feature = "cuda", feature = "gguf"))]

mod common;

use std::path::{Path, PathBuf};

use weaver_spu::readout::ReadoutElection;
use weaver_spu::residency::{Headroom, Residency};
use weaver_types::{ArtifactRef, DecoderInstruction, DeviceOrdinal, ModelBinding, SpuInstruction};

/// A small real model this workshop holds. The test is about the load path,
/// not the model, so the fixture is the smallest artifact whose architecture
/// and template agree. That second condition is not decoration: the smaller
/// smollm2-360m declares `llama` and renders ChatML, so the registry hands it
/// the Llama 3 stop set it was never trained against and the load refuses,
/// correctly. Until an entry carries that pairing the fixture cannot be chosen
/// on size alone. `WEAVER_TEST_GGUF` overrides the location for a machine laid
/// out differently.
const MODEL: &str = "/opt/weaver/models/qwen2.5-0.5b-instruct-q6_k.gguf";

/// A fixture whose family declares no residual tap, for the readout refusal.
///
/// **The refusal needs an untapping family and no longer accepts any GGUF.**
/// Until 2026-08-22 the container was itself a ground for refusing an elected
/// readout, so [`MODEL`] served that test as well as every other. The GGUF tap
/// stands as of that date and `qwen2` declares it, so [`MODEL`] is now
/// admitted under an election and the refusal has to be reached through a
/// family that declares nothing. SmolLM2 selects a `llama` entry, which
/// declares `taps_readout: false`, and it is the smallest artifact here that
/// does.
const UNTAPPED_MODEL: &str = "/opt/weaver/models/smollm2-360m-instruct-q8_0.gguf";

fn untapped_model_present() -> Option<PathBuf> {
    Path::new(UNTAPPED_MODEL)
        .is_file()
        .then(|| PathBuf::from(UNTAPPED_MODEL))
}

fn model_present() -> Option<PathBuf> {
    match std::env::var_os("WEAVER_TEST_GGUF") {
        // An explicit request that cannot be met is a failure rather than a
        // skip: a runner that asked for the load path is told it did not run,
        // instead of a green result claiming it did. The skip below survives
        // only for the implicit case, where this machine simply is not one
        // that holds the fixture. Both branches demand a regular file, since
        // a directory or a socket at the path would fail later and blame the
        // loader.
        Some(named) => {
            let path = PathBuf::from(named);
            assert!(
                path.is_file(),
                "WEAVER_TEST_GGUF names {}, which is not a regular file",
                path.display()
            );
            Some(path)
        }
        None => Path::new(MODEL).is_file().then(|| PathBuf::from(MODEL)),
    }
}

/// The device tests bind device 0 and read device-global free memory, so a
/// parallel run would have one test's admit move another test's measurement,
/// an intermittent failure about scheduling rather than about the property.
/// They take this lock and run one at a time.
static DEVICE: std::sync::Mutex<()> = std::sync::Mutex::new(());

fn device_lock() -> std::sync::MutexGuard<'static, ()> {
    // A poisoned lock means an earlier test panicked, which that test already
    // reported. Cascading its failure into this one would report it twice.
    DEVICE
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
}

/// One context held for a whole measurement sequence, so the probes read a
/// stable view: creating and dropping a context per probe would retain and
/// release the primary context around each read.
fn device_context() -> Option<std::sync::Arc<cudarc::driver::CudaContext>> {
    cudarc::driver::CudaContext::new(0).ok()
}

fn free_bytes(context: &cudarc::driver::CudaContext) -> u64 {
    let (free, _total) = context.mem_get_info().expect("the driver answers");
    free as u64
}

/// **Admit holds the weights on the device, and release frees them before it
/// answers.** The Spec carries the release ordering as review's, the instrument
/// being priced against a driver seam the Spec does not introduce. This build
/// has the driver itself, so the test reads the device: free memory drops
/// across the admit and returns across the release, which is the confirmation
/// being a fact about the device rather than a statement of intent.
///
/// The second-admit refusal is asserted inside the same lifecycle rather than
/// in a second test, and the shape is load-bearing: one process performs one
/// load, exactly as the shipped binary does. The engine hangs on a
/// free-then-reload inside one process, found when the device lock serialized
/// what the parallel run had overlapped, and the shipped binary cannot meet
/// that hang because this crate begins empty, admits once, and dies. A second
/// in-process lifecycle exists only in a harness, so the harness does not
/// build one.
///
/// Perturbation: make `Residency::release` answer `Ok` without taking the
/// resident and this test fails at the release assertion, because the weights
/// stay on the device. Watched under exactly that change.
#[test]
fn a_real_admit_holds_the_device_and_release_frees_it() {
    let Some(model) = model_present() else {
        eprintln!("SKIP a_real_admit_holds_the_device: no model at {MODEL}");
        return;
    };
    let Some(context) = device_context() else {
        eprintln!("SKIP a_real_admit_holds_the_device: no CUDA device");
        return;
    };
    let _device = device_lock();
    let before = free_bytes(&context);

    let mut residency = Residency::new();
    let binding = ModelBinding {
        artifact: ArtifactRef(model.to_string_lossy().into_owned()),
        devices: vec![DeviceOrdinal(0)],
    };

    let admitted = residency.admit(
        &binding,
        Headroom(64 * 1024 * 1024),
        ReadoutElection(false),
        false,
    );
    let resident = match admitted {
        Ok(resident) => resident,
        Err(refusal) => panic!("the admit succeeds against a real artifact, got {refusal:?}"),
    };

    // The weights hash is real, never the sentinel, against bytes that loaded.
    assert!(
        !resident.weights_hash.is_sentinel(),
        "a loaded artifact hashes to a value"
    );
    let held = free_bytes(&context);
    // The artifact is ~370 MB quantized. The device must be holding a
    // substantial part of that; the threshold is far below the artifact size
    // so allocator granularity cannot flake it, while far above noise.
    assert!(
        before.saturating_sub(held) > 100 * 1024 * 1024,
        "the admit holds device memory: before {before}, held {held}"
    );

    // Nothing is idempotent: a second admit refuses on the ordering with an
    // identical binding, while the first residency stands.
    let second = residency.admit(
        &binding,
        Headroom(64 * 1024 * 1024),
        ReadoutElection(false),
        false,
    );
    assert!(
        matches!(
            second,
            Err(weaver_spu::residency::AdmitRefusal::AlreadyAttempted)
        ),
        "the second admit refuses on the ordering even with an identical binding"
    );

    residency.release().expect("the release succeeds");
    let after = free_bytes(&context);
    assert!(
        after.saturating_sub(held) > 100 * 1024 * 1024,
        "the release returns the memory before answering: held {held}, after {after}"
    );
}

mod seam_success {
    //! The same round trip crossing the real socket into the real binary,
    //! which is what the contract governs: an answer to admit arrives only
    //! after the device holds the weights, and an answer to release only after
    //! the device is free.

    use super::*;
    use std::os::fd::AsRawFd;
    use std::process::{Command, Stdio};

    use crate::common::{ask, bound_receives, place_inherited, seqpacket_pair, wait_bounded};
    use weaver_types::{LifecycleAnswer, LifecycleDirective, LifecycleRefusal, Payload};

    /// **The election the wire carries is the election the judgment
    /// receives.** An admit whose instruction elects readout against a family
    /// that declares no tap refuses at admit, per Spec section 7 and charter
    /// step 3. The refusal crosses as `DeviceCannotAdmit`, the floor case the
    /// election failure maps onto, and it lands at the third step, so the
    /// device this file otherwise requires is never touched on this path.
    ///
    /// **The fixture moved when the ground did.** This read the refusal
    /// against [`MODEL`] while the container was a ground and every GGUF load
    /// refused an election. The GGUF tap stood on 2026-08-22 and `qwen2`
    /// declares it, so [`MODEL`] is admitted under an election now and the
    /// refusal is reached through [`UNTAPPED_MODEL`], whose family declares
    /// nothing. **That the fixture had to move is the point**: the test was
    /// asserting a property of the container and now asserts one of the
    /// declaration, which is where the judgment always was in code.
    ///
    /// Perturbation: replace the dispatched election at the admit arm with
    /// `ReadoutElection(false)`, the placeholder the routeless seam once
    /// forced, and this test fails on this machine, the admit proceeding past
    /// the judgment onto the device and answering `Admitted`. Watched under
    /// exactly that change, which is why the device skip stays: without a
    /// device the perturbed run refuses later for other reasons and the watch
    /// distinguishes nothing.
    #[test]
    fn an_admit_electing_readout_refuses_across_the_seam() {
        let Some(model) = untapped_model_present() else {
            eprintln!("SKIP an_admit_electing_readout_refuses: no model at {UNTAPPED_MODEL}");
            return;
        };
        if device_context().is_none() {
            eprintln!("SKIP an_admit_electing_readout_refuses: no CUDA device");
            return;
        }
        let _device = device_lock();

        let (lifecycle, child_lifecycle) = seqpacket_pair();
        let (_decode, child_decode) = seqpacket_pair();
        let mut command = Command::new(env!("CARGO_BIN_EXE_weaver-spu"));
        command
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null());
        place_inherited(
            &mut command,
            &[child_lifecycle.as_raw_fd(), child_decode.as_raw_fd()],
        );
        let mut child = command.spawn().expect("the binary starts");
        bound_receives(&lifecycle, 120);

        let refused = ask(
            &lifecycle,
            1,
            LifecycleDirective::Admit {
                instruction: SpuInstruction {
                    classify: None,
                    decoder: DecoderInstruction {
                        model_binding: ModelBinding {
                            artifact: ArtifactRef(model.to_string_lossy().into_owned()),
                            devices: vec![DeviceOrdinal(0)],
                        },
                        residual_readout_election: true,
                        field_election: None,
                        surprisal_election: false,
                        refeed_permission: false,
                        column_permission: false,
                        identity: vec![],
                        tunable_values: [
                            ("max-tokens-per-turn".to_string(), 4096.0),
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
            refused.payload,
            Payload::Refusal(LifecycleRefusal::DeviceCannotAdmit),
            "an elected readout a family does not declare refuses at admit"
        );
        assert_eq!(refused.exchange.ordinal, 1, "on the exchange that asked");

        drop(lifecycle);
        let status = wait_bounded(&mut child, 30, "the refused worker exits");
        assert!(
            status.success(),
            "a refused admit is an answer, not a death: the worker serves on and \
             exits clean when the channel closes"
        );
    }

    /// **The contract's success path, whole:** admit answers `Admitted`,
    /// release answers `Released`, each on the exchange that asked, across the
    /// real seam, against a real artifact, on a real device.
    ///
    /// Every wait is bounded. The receives carry a timeout sized to a model
    /// load, and the child's exit is polled against a deadline with a kill on
    /// expiry, so a wedged stage becomes a failure naming itself rather than a
    /// hang the harness cannot end.
    #[test]
    fn admit_then_release_round_trips_across_the_seam() {
        let Some(model) = model_present() else {
            eprintln!("SKIP admit_then_release_round_trips: no model at {MODEL}");
            return;
        };
        if device_context().is_none() {
            eprintln!("SKIP admit_then_release_round_trips: no CUDA device");
            return;
        }
        // Held across the whole spawn-and-wait, because the child process is
        // what takes the device.
        let _device = device_lock();

        let (lifecycle, child_lifecycle) = seqpacket_pair();
        let (decode_parent, child_decode) = seqpacket_pair();
        let mut command = Command::new(env!("CARGO_BIN_EXE_weaver-spu"));
        // The streams go to a file rather than pipes: llama.cpp writes model
        // metadata to stderr during the load, nothing here drains a pipe until
        // after the exchanges, and a load chatty enough to fill the buffer
        // would wedge the child mid-write against the parent mid-recv. A file
        // has no such buffer, and unlike null it leaves the child's account on
        // disk for the failure that needs it.
        let log_path =
            std::env::temp_dir().join(format!("weaver-spu-seam-child-{}.log", std::process::id()));
        let log = std::fs::File::create(&log_path).expect("a child log file");
        command
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::from(log));
        place_inherited(
            &mut command,
            &[child_lifecycle.as_raw_fd(), child_decode.as_raw_fd()],
        );
        let mut child = command.spawn().expect("the binary starts");
        // Generous against a model load, far under the harness's patience.
        bound_receives(&lifecycle, 120);

        let admitted = ask(
            &lifecycle,
            1,
            LifecycleDirective::Admit {
                instruction: SpuInstruction {
                    classify: None,
                    decoder: DecoderInstruction {
                        model_binding: ModelBinding {
                            artifact: ArtifactRef(model.to_string_lossy().into_owned()),
                            devices: vec![DeviceOrdinal(0)],
                        },
                        residual_readout_election: false,
                        field_election: None,
                        surprisal_election: false,
                        refeed_permission: false,
                        column_permission: false,
                        identity: vec![],
                        tunable_values: [
                            ("max-tokens-per-turn".to_string(), 4096.0),
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
            "the admit answers Admitted across the seam"
        );
        assert_eq!(admitted.exchange.ordinal, 1);

        // The decode end closes before the release is asked: once the admit
        // confirms, the worker's decode phase owns it until the channel ends,
        // per the serve loop's one-loop rule, so a release sent while the
        // decode end stands would wait behind a phase that never returns.
        drop(decode_parent);

        let released = ask(&lifecycle, 2, LifecycleDirective::Release);
        assert_eq!(
            released.payload,
            Payload::Answer(LifecycleAnswer::Released),
            "the release answers Released across the seam"
        );
        assert_eq!(released.exchange.ordinal, 2);

        drop(lifecycle);
        let status = wait_bounded(
            &mut child,
            30,
            &format!(
                "admit_then_release_round_trips, child stderr at {}",
                log_path.display()
            ),
        );
        assert!(status.success(), "and the process exits clean");
        std::fs::remove_file(&log_path).ok();
    }

    /// **The seam serves, end to end:** an open renders and makes the prefix
    /// resident, an append renders the turn, generates against the real
    /// engine, and answers with the generation and its measurement, a flush
    /// returns the session to its prefix, and the whole conversation crosses
    /// the real decode socket as the trio's JSON. This is the decode seam's
    /// service act watched whole, per `weaver-harness-spu-decode-contract`
    /// section 2 and `weaver-spu-Spec` section 9.
    #[test]
    fn the_declared_ceiling_governs_the_generation() {
        use weaver_traits::{ContentBlock, Message, Role};
        use weaver_types::{SessionId, TokenAnswer, TokenDirective, TurnKey};

        let Some(model) = model_present() else {
            eprintln!("SKIP the_declared_ceiling: no model at {MODEL}");
            return;
        };
        if device_context().is_none() {
            eprintln!("SKIP the_declared_ceiling: no CUDA device");
            return;
        }
        let _device = device_lock();

        let (lifecycle, child_lifecycle) = seqpacket_pair();
        let (decode_parent, child_decode) = seqpacket_pair();
        let log_path = std::env::temp_dir().join(format!(
            "weaver-spu-ceiling-child-{}.log",
            std::process::id()
        ));
        let log = std::fs::File::create(&log_path).expect("a child log file");
        let mut command = Command::new(env!("CARGO_BIN_EXE_weaver-spu"));
        command
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::from(log));
        place_inherited(
            &mut command,
            &[child_lifecycle.as_raw_fd(), child_decode.as_raw_fd()],
        );
        let mut child = command.spawn().expect("the binary starts");
        bound_receives(&lifecycle, 120);
        bound_receives(&decode_parent, 120);
        let decode = weaver_spu::channel::decode_from_owned(decode_parent);

        // The declaration elects a ceiling far below any natural stop, so
        // the only way the count below comes out right is the declared
        // value reaching the generation's stop condition: a test at 4096
        // would pass whenever a short answer finished naturally, proving
        // only that the knob did not refuse.
        let admitted = ask(
            &lifecycle,
            1,
            LifecycleDirective::Admit {
                instruction: SpuInstruction {
                    classify: None,
                    decoder: DecoderInstruction {
                        model_binding: ModelBinding {
                            artifact: ArtifactRef(model.to_string_lossy().into_owned()),
                            devices: vec![DeviceOrdinal(0)],
                        },
                        residual_readout_election: false,
                        field_election: None,
                        surprisal_election: false,
                        refeed_permission: false,
                        column_permission: false,
                        identity: vec![],
                        tunable_values: [
                            ("max-tokens-per-turn".to_string(), 9.0),
                            ("context-capacity".to_string(), 4096.0),
                            ("seed".to_string(), 11.0),
                        ]
                        .into_iter()
                        .collect(),
                    },
                },
            },
        );
        assert_eq!(admitted.payload, Payload::Answer(LifecycleAnswer::Admitted));

        let message = |text: &str| Message {
            role: Role::User,
            content: vec![ContentBlock::Text { text: text.into() }],
        };
        let send = |directive: &TokenDirective| {
            let body = serde_json::to_vec(directive).expect("a directive renders");
            decode.send_octets(&body).expect("the frame sends");
        };
        let recv = || -> TokenAnswer {
            let frame = decode.recv_octets().expect("an answer arrives");
            serde_json::from_slice(&frame).expect("the answer parses")
        };

        send(&TokenDirective::Open {
            session: SessionId("s-ceiling".into()),
            column_ask: false,
            messages: vec![message("You count plainly.")],
        });
        assert_eq!(recv(), TokenAnswer::Opened, "the session opens");

        send(&TokenDirective::AppendAndGenerate {
            turn: TurnKey("t-1".into()),
            delta: vec![message(
                "Count upward from one, comma separated, without stopping.",
            )],
        });
        let mut streamed = 0usize;
        let generation = loop {
            match recv() {
                TokenAnswer::Token { .. } => streamed += 1,
                TokenAnswer::Generated(generation) => break generation,
                other => panic!("tokens then the close, got {other:?}"),
            }
        };
        assert_eq!(
            streamed, 9,
            "the declared nine-token ceiling governed the generation"
        );
        let measurement: serde_json::Value =
            serde_json::from_str(generation.measurement.get()).expect("measurement is JSON");
        assert_eq!(
            measurement["output_tokens"]
                .as_array()
                .expect("output tokens")
                .len(),
            9,
            "the measurement agrees with the stream"
        );

        drop(decode);
        drop(lifecycle);
        let _ = child.wait();
        let _ = std::fs::remove_file(&log_path);
    }

    /// **An elected surprisal reaches the wire, and the perplexity stands
    /// beside it rather than instead of it.** The declined posture is
    /// pinned by the seam test below, which is this suite's ordinary
    /// fixture, so what this adds is the other state: the election has to
    /// be watched in both, a flag that is always false passing a test that
    /// never sets it true.
    ///
    /// The election is fixed at admit for the residency's life, per
    /// `weaver-spu-PRD` section 13.12, so a second admit is what it takes
    /// to see the other arm rather than a second directive.
    #[test]
    fn an_elected_surprisal_reaches_the_wire() {
        use weaver_traits::{ContentBlock, Message, Role};
        use weaver_types::{SessionId, TokenAnswer, TokenDirective, TurnKey};

        let Some(model) = model_present() else {
            eprintln!("SKIP an_elected_surprisal: no model at {MODEL}");
            return;
        };
        if device_context().is_none() {
            eprintln!("SKIP an_elected_surprisal: no CUDA device");
            return;
        }
        let _device = device_lock();

        let (lifecycle, child_lifecycle) = seqpacket_pair();
        let (decode_parent, child_decode) = seqpacket_pair();
        let log_path = std::env::temp_dir().join(format!(
            "weaver-spu-surprisal-child-{}.log",
            std::process::id()
        ));
        let log = std::fs::File::create(&log_path).expect("a child log file");
        let mut command = Command::new(env!("CARGO_BIN_EXE_weaver-spu"));
        command
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::from(log));
        place_inherited(
            &mut command,
            &[child_lifecycle.as_raw_fd(), child_decode.as_raw_fd()],
        );
        let mut child = command.spawn().expect("the binary starts");
        bound_receives(&lifecycle, 120);
        bound_receives(&decode_parent, 120);
        let decode = weaver_spu::channel::decode_from_owned(decode_parent);

        // The declaration elects the surprisal, which is the one thing
        // that differs from the seam test below: the ceiling, the model,
        // and the directives are that test's, so a difference in the
        // measurement is the election's doing.
        let admitted = ask(
            &lifecycle,
            1,
            LifecycleDirective::Admit {
                instruction: SpuInstruction {
                    classify: None,
                    decoder: DecoderInstruction {
                        model_binding: ModelBinding {
                            artifact: ArtifactRef(model.to_string_lossy().into_owned()),
                            devices: vec![DeviceOrdinal(0)],
                        },
                        residual_readout_election: false,
                        field_election: None,
                        surprisal_election: true,
                        refeed_permission: false,
                        column_permission: false,
                        identity: vec![],
                        tunable_values: [
                            ("max-tokens-per-turn".to_string(), 9.0),
                            ("context-capacity".to_string(), 4096.0),
                            ("seed".to_string(), 11.0),
                        ]
                        .into_iter()
                        .collect(),
                    },
                },
            },
        );
        assert_eq!(admitted.payload, Payload::Answer(LifecycleAnswer::Admitted));

        let message = |text: &str| Message {
            role: Role::User,
            content: vec![ContentBlock::Text { text: text.into() }],
        };
        let send = |directive: &TokenDirective| {
            let body = serde_json::to_vec(directive).expect("a directive renders");
            decode.send_octets(&body).expect("the frame sends");
        };
        let recv = || -> TokenAnswer {
            let frame = decode.recv_octets().expect("an answer arrives");
            serde_json::from_slice(&frame).expect("the answer parses")
        };

        send(&TokenDirective::Open {
            session: SessionId("s-surprisal".into()),
            column_ask: false,
            messages: vec![message("You count plainly.")],
        });
        assert_eq!(recv(), TokenAnswer::Opened, "the session opens");

        send(&TokenDirective::AppendAndGenerate {
            turn: TurnKey("t-1".into()),
            delta: vec![message(
                "Count upward from one, comma separated, without stopping.",
            )],
        });
        let mut streamed = 0usize;
        let generation = loop {
            match recv() {
                TokenAnswer::Token { .. } => streamed += 1,
                TokenAnswer::Generated(generation) => break generation,
                other => panic!("tokens then the close, got {other:?}"),
            }
        };
        assert!(streamed > 0, "the model answered across the seam");
        let measurement: serde_json::Value =
            serde_json::from_str(generation.measurement.get()).expect("measurement is JSON");
        // **The elected posture on the wire**, the other half of what
        // `an_opened_session_generates_across_the_decode_seam` pins for the
        // declined one. Both members stand, and they are paired with the
        // tokens position for position, which is the property that makes a
        // per-position reading readable at all.
        let tokens = measurement["output_tokens"]
            .as_array()
            .expect("output tokens")
            .len();
        let surprisals = measurement["surprisals"]
            .as_array()
            .expect("the elected vector is on the wire")
            .len();
        assert_eq!(
            surprisals, tokens,
            "the vector is paired with the tokens: {measurement}"
        );
        assert_eq!(
            measurement["entropies"].as_array().map(Vec::len),
            Some(tokens),
            "and so is the entropy, which carries no election"
        );
        assert!(
            measurement["perplexity"].is_number(),
            "the perplexity stands beside the vector rather than instead \
             of it: {measurement}"
        );

        drop(decode);
        drop(lifecycle);
        let _ = child.wait();
        let _ = std::fs::remove_file(&log_path);
    }

    #[test]
    fn an_opened_session_generates_across_the_decode_seam() {
        use weaver_traits::{ContentBlock, Message, Role};
        use weaver_types::{SessionId, TokenAnswer, TokenDirective, TurnKey};

        let Some(model) = model_present() else {
            eprintln!("SKIP an_opened_session_generates: no model at {MODEL}");
            return;
        };
        if device_context().is_none() {
            eprintln!("SKIP an_opened_session_generates: no CUDA device");
            return;
        }
        let _device = device_lock();

        let (lifecycle, child_lifecycle) = seqpacket_pair();
        let (decode_parent, child_decode) = seqpacket_pair();
        let log_path = std::env::temp_dir().join(format!(
            "weaver-spu-decode-child-{}.log",
            std::process::id()
        ));
        let log = std::fs::File::create(&log_path).expect("a child log file");
        let mut command = Command::new(env!("CARGO_BIN_EXE_weaver-spu"));
        command
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::from(log));
        place_inherited(
            &mut command,
            &[child_lifecycle.as_raw_fd(), child_decode.as_raw_fd()],
        );
        let mut child = command.spawn().expect("the binary starts");
        bound_receives(&lifecycle, 120);
        bound_receives(&decode_parent, 120);
        let decode = weaver_spu::channel::decode_from_owned(decode_parent);

        let admitted = ask(
            &lifecycle,
            1,
            LifecycleDirective::Admit {
                instruction: SpuInstruction {
                    classify: None,
                    decoder: DecoderInstruction {
                        model_binding: ModelBinding {
                            artifact: ArtifactRef(model.to_string_lossy().into_owned()),
                            devices: vec![DeviceOrdinal(0)],
                        },
                        residual_readout_election: false,
                        field_election: None,
                        surprisal_election: false,
                        refeed_permission: false,
                        column_permission: false,
                        identity: vec![],
                        tunable_values: [
                            ("max-tokens-per-turn".to_string(), 4096.0),
                            ("context-capacity".to_string(), 4096.0),
                            ("seed".to_string(), 37.0),
                        ]
                        .into_iter()
                        .collect(),
                    },
                },
            },
        );
        assert_eq!(admitted.payload, Payload::Answer(LifecycleAnswer::Admitted));

        let message = |text: &str| Message {
            role: Role::User,
            content: vec![ContentBlock::Text { text: text.into() }],
        };
        let send = |directive: &TokenDirective| {
            let body = serde_json::to_vec(directive).expect("a directive renders");
            decode.send_octets(&body).expect("the frame sends");
        };
        let recv = || -> TokenAnswer {
            let frame = decode.recv_octets().expect("an answer arrives");
            serde_json::from_slice(&frame).expect("the answer parses")
        };

        send(&TokenDirective::Open {
            session: SessionId("s-1".into()),
            column_ask: false,
            messages: vec![message("You answer in as few words as possible.")],
        });
        assert_eq!(recv(), TokenAnswer::Opened, "the session opens");

        send(&TokenDirective::AppendAndGenerate {
            turn: TurnKey("t-1".into()),
            delta: vec![message("Say one word.")],
        });
        // **The stream precedes the close and never disagrees with it**, per
        // the contract's coherence guarantee: token frames arrive as drawn,
        // the pieces accumulate to the emission, and the close carries the
        // generation whole.
        let mut pieces = String::new();
        let mut streamed = 0usize;
        let generation = loop {
            match recv() {
                TokenAnswer::Token { piece, .. } => {
                    pieces.push_str(&piece);
                    streamed += 1;
                }
                TokenAnswer::Generated(generation) => break generation,
                other => panic!("the stream carries tokens then the close, got {other:?}"),
            }
        };
        assert!(
            !generation.emission.is_empty(),
            "a real model said something"
        );
        assert!(streamed > 0, "the seam streamed rather than batched");
        assert_eq!(
            pieces, generation.emission,
            "the pieces accumulate to the emission, stream and close agreeing"
        );
        let request: serde_json::Value =
            serde_json::from_str(generation.request.get()).expect("the request splice is JSON");
        eprintln!("DIAG-RENDERED: {}", request["rendered"]);
        eprintln!("DIAG-EMISSION: {:?}", generation.emission);
        assert!(
            request["rendered"]
                .as_str()
                .is_some_and(|r| r.contains("Say one word.")),
            "the request carries the family's render of the delta"
        );
        assert!(
            request["template"].as_str().is_some(),
            "and the template identity"
        );
        assert!(
            request["sampling"]["temperature"].is_number(),
            "and the effective sampling"
        );
        // **The declared seed reaches the effective sampling**, which is the
        // whole of the tunable path added 2026-08-20: the value this fixture
        // supplies is the value the request reports, so a recorded run names
        // the draw it was and is re-entered by declaring it again.
        //
        // **The value is 37 because it must not be 11.** Eleven was the
        // compiled seed this knob carried while it was frozen, so a fixture
        // declaring eleven would pass against a still-frozen knob and prove
        // nothing about the route. Restore `Disposition::Frozen(11)` and
        // this fails, which is the perturbation, and it only fails because
        // the two numbers differ.
        assert_eq!(
            request["sampling"]["seed"], 37.0,
            "the declared seed is the effective one"
        );
        let measurement: serde_json::Value =
            serde_json::from_str(generation.measurement.get()).expect("the measurement is JSON");
        assert_eq!(
            measurement["blocks"][0]["label"], "turn-delta",
            "the block carries the declared label"
        );
        assert!(
            measurement["output_tokens"]
                .as_array()
                .is_some_and(|tokens| !tokens.is_empty()),
            "the tokens out are in the measurement"
        );
        assert!(
            measurement["timings"]["decode_ns"].is_string(),
            "the timings ride the decimal-string rule"
        );

        // **The unelected posture on the wire**, per `weaver-spu-PRD`
        // section 13.12: this fixture declares no surprisal election, so
        // the vector is absent and the perplexity stands in its place. The
        // entropies carry no election and are here either way.
        //
        // **The absence is checked as an absent member rather than a null.**
        // A member present and carrying nothing is the empty vector's
        // defect wearing another shape, and it is what an unguarded
        // serialization would produce.
        assert!(
            measurement.get("surprisals").is_none(),
            "no election, no vector, and no member for it: {measurement}"
        );
        assert!(
            measurement["perplexity"].is_number(),
            "the perplexity stands in the vector's place: {measurement}"
        );
        assert!(
            measurement["entropies"]
                .as_array()
                .is_some_and(|bits| !bits.is_empty()),
            "and the entropy carries no election at all"
        );
        assert_eq!(
            measurement["entropies"].as_array().map(Vec::len),
            measurement["output_tokens"].as_array().map(Vec::len),
            "the entropy is paired with the tokens position for position"
        );

        send(&TokenDirective::Flush { keep: 0 });
        let flushed = recv();
        let TokenAnswer::Flushed {
            resident_before,
            resident_after,
        } = flushed
        else {
            panic!("the flush reaches its outcome: {flushed:?}");
        };
        assert!(
            resident_before > resident_after,
            "the flush cut what stood: {resident_before} to {resident_after}"
        );

        drop(decode);
        let released = ask(&lifecycle, 2, LifecycleDirective::Release);
        assert_eq!(released.payload, Payload::Answer(LifecycleAnswer::Released));

        drop(lifecycle);
        let status = wait_bounded(
            &mut child,
            30,
            &format!(
                "the served worker exits, child stderr at {}",
                log_path.display()
            ),
        );
        assert!(status.success(), "and the process exits clean");
        std::fs::remove_file(&log_path).ok();
    }

    /// **A serving load never opens the re-feed drive.** The permission is
    /// admin's member, set from the binding's kind and never the
    /// declaration's, so an instruction without it meets the registry's
    /// first arm before the session is touched, per `weaver-spu-PRD`
    /// section 13.14. This is the structural read of "we should never have
    /// a buggy serving harness within reach of the drive": the refusal is
    /// the seam's, not the caller's manners.
    ///
    /// Perturbation: remove the permission check from the `ReFeed` dispatch
    /// arm and this fails, the drive running and answering `ReFed` against
    /// a serving instruction. Watched under exactly that removal.
    ///
    /// conforms: spu-refeed-permission-judged-first
    #[test]
    fn a_serving_load_never_opens_the_refeed_drive() {
        use weaver_traits::{ContentBlock, Message, Role};
        use weaver_types::{SessionId, TokenAnswer, TokenDirective, TokenRefusal, TurnKey};

        let Some(model) = model_present() else {
            eprintln!("SKIP a_serving_load_never_opens: no model at {MODEL}");
            return;
        };
        if device_context().is_none() {
            eprintln!("SKIP a_serving_load_never_opens: no CUDA device");
            return;
        }
        let _device = device_lock();

        let (lifecycle, child_lifecycle) = seqpacket_pair();
        let (decode_parent, child_decode) = seqpacket_pair();
        let log_path = std::env::temp_dir().join(format!(
            "weaver-spu-refeed-refusal-child-{}.log",
            std::process::id()
        ));
        let log = std::fs::File::create(&log_path).expect("a child log file");
        let mut command = Command::new(env!("CARGO_BIN_EXE_weaver-spu"));
        command
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::from(log));
        place_inherited(
            &mut command,
            &[child_lifecycle.as_raw_fd(), child_decode.as_raw_fd()],
        );
        let mut child = command.spawn().expect("the binary starts");
        bound_receives(&lifecycle, 120);
        bound_receives(&decode_parent, 120);
        let decode = weaver_spu::channel::decode_from_owned(decode_parent);

        let admitted = ask(
            &lifecycle,
            1,
            LifecycleDirective::Admit {
                instruction: SpuInstruction {
                    classify: None,
                    decoder: DecoderInstruction {
                        model_binding: ModelBinding {
                            artifact: ArtifactRef(model.to_string_lossy().into_owned()),
                            devices: vec![DeviceOrdinal(0)],
                        },
                        residual_readout_election: false,
                        field_election: None,
                        surprisal_election: false,
                        refeed_permission: false,
                        column_permission: false,
                        identity: vec![],
                        tunable_values: [
                            ("max-tokens-per-turn".to_string(), 64.0),
                            ("context-capacity".to_string(), 1024.0),
                            ("seed".to_string(), 37.0),
                        ]
                        .into_iter()
                        .collect(),
                    },
                },
            },
        );
        assert_eq!(admitted.payload, Payload::Answer(LifecycleAnswer::Admitted));

        let send = |directive: &TokenDirective| {
            let body = serde_json::to_vec(directive).expect("a directive renders");
            decode.send_octets(&body).expect("the frame sends");
        };
        let recv = || -> TokenAnswer {
            let frame = decode.recv_octets().expect("an answer arrives");
            serde_json::from_slice(&frame).expect("the answer parses")
        };

        send(&TokenDirective::Open {
            session: SessionId("s-refused".into()),
            column_ask: false,
            messages: vec![Message {
                role: Role::User,
                content: vec![ContentBlock::Text {
                    text: "You answer in as few words as possible.".into(),
                }],
            }],
        });
        assert_eq!(recv(), TokenAnswer::Opened, "the session opens");

        send(&TokenDirective::ReFeed {
            turn: TurnKey("t-1".into()),
            rendered: "anything".into(),
            path: vec![1],
        });
        // A refusal crosses as the trio's own refusal frame, not an answer.
        let frame = decode.recv_octets().expect("the refusal arrives");
        let refusal: TokenRefusal = serde_json::from_slice(&frame).expect("the refusal parses");
        assert_eq!(
            refusal,
            TokenRefusal::RefeedPermissionAbsent,
            "the drive refuses on the registry's first arm"
        );

        drop(decode);
        let released = ask(&lifecycle, 2, LifecycleDirective::Release);
        assert_eq!(released.payload, Payload::Answer(LifecycleAnswer::Released));
        drop(lifecycle);
        let status = wait_bounded(
            &mut child,
            30,
            &format!(
                "the refused worker exits, child stderr at {}",
                log_path.display()
            ),
        );
        assert!(status.success(), "and the process exits clean");
        std::fs::remove_file(&log_path).ok();
    }

    /// **A cancelled re-feed certifies nothing and faults across the seam.**
    /// The drive is handed the whole recorded path upfront, so the text it
    /// owes is the record's while the measurement is the run's, and a cancel
    /// parts them. What would be left to say is a prefix, which is the nothing
    /// the registry's second arm already refuses, so the seam carries a fault
    /// rather than a `ReFed` whose text covers positions the run never
    /// reached, per the operator's ruling of 2026-09-14.
    ///
    /// **The cancel is queued behind the directive rather than raced against
    /// it.** Both frames are sent before the drive starts, so the poll at the
    /// top of the first position finds the cancel already waiting and the stop
    /// lands at zero. Nothing here depends on how fast the device is.
    ///
    /// **The certificate is what is withheld, not every frame.** This admit
    /// elects no field and asks no column, so nothing streams from the sinks
    /// and the socket is silent until the fault. Where a field or column is
    /// elected the intermediates for the positions that did become resident
    /// have already crossed, per `weaver-harness-Spec` section 6 and
    /// `weaver-diagnostic-Spec` section 3.2, which have them author as they
    /// arrive. They are true of what ran. This test reads the certificate's
    /// absence and says so rather than claiming a silence it arranged.
    ///
    /// **This is the instrument the unit test cannot be.** The guard is in the
    /// binary, so `session.rs`'s cancelled re-feed test pins what the session
    /// does and not what the caller decides about it. Removing the
    /// `cancel.cancelled` term from that guard leaves the unit test green and
    /// this one red, which is the whole reason it is here.
    ///
    /// Perturbation: drop `cancel.cancelled` from the re-feed arm's guard and
    /// the process answers `ReFed` and exits clean. Watched under exactly that
    /// removal.
    #[test]
    fn a_cancelled_refeed_faults_and_certifies_nothing() {
        use weaver_traits::{ContentBlock, Message, Role};
        use weaver_types::{SessionId, TokenAnswer, TokenDirective, TurnKey};

        let Some(model) = model_present() else {
            eprintln!("SKIP a_cancelled_refeed_faults: no model at {MODEL}");
            return;
        };
        if device_context().is_none() {
            eprintln!("SKIP a_cancelled_refeed_faults: no CUDA device");
            return;
        }
        let _device = device_lock();

        let (lifecycle, child_lifecycle) = seqpacket_pair();
        let (decode_parent, child_decode) = seqpacket_pair();
        let log_path = std::env::temp_dir().join(format!(
            "weaver-spu-refeed-cancel-child-{}.log",
            std::process::id()
        ));
        let log = std::fs::File::create(&log_path).expect("a child log file");
        let mut command = Command::new(env!("CARGO_BIN_EXE_weaver-spu"));
        command
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::from(log));
        place_inherited(
            &mut command,
            &[child_lifecycle.as_raw_fd(), child_decode.as_raw_fd()],
        );
        let mut child = command.spawn().expect("the binary starts");
        bound_receives(&lifecycle, 120);
        bound_receives(&decode_parent, 120);
        let decode = weaver_spu::channel::decode_from_owned(decode_parent);

        let admitted = ask(
            &lifecycle,
            1,
            LifecycleDirective::Admit {
                instruction: SpuInstruction {
                    classify: None,
                    decoder: DecoderInstruction {
                        model_binding: ModelBinding {
                            artifact: ArtifactRef(model.to_string_lossy().into_owned()),
                            devices: vec![DeviceOrdinal(0)],
                        },
                        residual_readout_election: false,
                        field_election: None,
                        surprisal_election: false,
                        refeed_permission: true,
                        column_permission: false,
                        identity: vec![],
                        tunable_values: [
                            ("max-tokens-per-turn".to_string(), 64.0),
                            ("context-capacity".to_string(), 1024.0),
                            ("seed".to_string(), 37.0),
                        ]
                        .into_iter()
                        .collect(),
                    },
                },
            },
        );
        assert_eq!(admitted.payload, Payload::Answer(LifecycleAnswer::Admitted));

        let send = |directive: &TokenDirective| {
            let body = serde_json::to_vec(directive).expect("a directive renders");
            decode.send_octets(&body).expect("the frame sends");
        };

        send(&TokenDirective::Open {
            session: SessionId("s-cancelled".into()),
            column_ask: false,
            messages: vec![Message {
                role: Role::User,
                content: vec![ContentBlock::Text {
                    text: "You answer in as few words as possible.".into(),
                }],
            }],
        });
        let opened: TokenAnswer =
            serde_json::from_slice(&decode.recv_octets().expect("the open answers"))
                .expect("the answer parses");
        assert_eq!(opened, TokenAnswer::Opened, "the session opens");

        // Both frames before the drive starts, so the first poll finds the
        // cancel rather than racing the device for it.
        send(&TokenDirective::ReFeed {
            turn: TurnKey("t-1".into()),
            rendered: "anything".into(),
            path: vec![1, 2, 3],
        });
        send(&TokenDirective::Cancel {
            turn: TurnKey("t-1".into()),
        });

        // **No certificate crosses.** A read either fails on the closed socket
        // or, where the peer's exit has not landed yet, returns no frame at
        // all. Nothing else is in flight here, this admit electing no field
        // and asking no column, so any frame at all is the answer that must
        // not arrive.
        match decode.recv_octets() {
            Err(_) => {}
            Ok(frame) => panic!(
                "a cancelled re-feed answered instead of faulting: {}",
                String::from_utf8_lossy(&frame)
            ),
        }

        drop(decode);
        drop(lifecycle);
        let status = wait_bounded(
            &mut child,
            30,
            &format!(
                "the cancelled worker exits, child stderr at {}",
                log_path.display()
            ),
        );
        assert!(
            !status.success(),
            "the fault is the exit code as well as the line"
        );
        let said = std::fs::read_to_string(&log_path).unwrap_or_default();
        assert!(
            said.contains("\"fault\":\"undecodable\""),
            "the fault names itself on stderr, which held: {said}"
        );
        std::fs::remove_file(&log_path).ok();
    }

    /// **The null re-feed recomputes the recorded draws exactly.** Process
    /// one is the source: a real generation against the real engine, its
    /// rendered form and token path captured from its own answer. Process
    /// two is the replay: a fresh residency of the same declaration,
    /// granted the permission, re-feeding the recorded rendered form along
    /// the recorded path. The recomputed identifiers in the measurement's
    /// output slots must equal the recorded path integer for integer, per
    /// `weaver-spu-PRD` section 13.14 and the certification of
    /// `diagnostic-replay-loop` section 3 - the same derived seed from the
    /// same turn key and ordinal against the same weights on the same
    /// device, which is `weaver-agents-PRD` section 8's reproducible claim
    /// exercised across two processes. The empty-path refusal, the
    /// registry's second arm, is read on the way.
    ///
    /// Perturbation for the second arm: remove the empty-path check from
    /// the dispatch arm and the empty re-feed reaches the session, watched
    /// red under that removal.
    ///
    /// conforms: spu-refeed-empty-path-refused
    /// conforms: spu-refeed-recomputes-the-recorded-draws
    #[test]
    fn the_null_refeed_recomputes_the_recorded_draws_exactly() {
        use weaver_traits::{ContentBlock, Message, Role};
        use weaver_types::{SessionId, TokenAnswer, TokenDirective, TokenRefusal, TurnKey};

        let Some(model) = model_present() else {
            eprintln!("SKIP the_null_refeed_recomputes: no model at {MODEL}");
            return;
        };
        if device_context().is_none() {
            eprintln!("SKIP the_null_refeed_recomputes: no CUDA device");
            return;
        }
        let _device = device_lock();

        let identity = || {
            vec![Message {
                role: Role::User,
                content: vec![ContentBlock::Text {
                    text: "You answer in as few words as possible.".into(),
                }],
            }]
        };
        let instruction = |refeed_permission: bool| SpuInstruction {
            classify: None,
            decoder: DecoderInstruction {
                model_binding: ModelBinding {
                    artifact: ArtifactRef(model.to_string_lossy().into_owned()),
                    devices: vec![DeviceOrdinal(0)],
                },
                residual_readout_election: false,
                field_election: None,
                surprisal_election: false,
                refeed_permission,
                column_permission: false,
                identity: vec![],
                tunable_values: [
                    ("max-tokens-per-turn".to_string(), 64.0),
                    ("context-capacity".to_string(), 1024.0),
                    ("seed".to_string(), 37.0),
                ]
                .into_iter()
                .collect(),
            },
        };
        let stand = |name: &str, permission: bool| {
            let (lifecycle, child_lifecycle) = seqpacket_pair();
            let (decode_parent, child_decode) = seqpacket_pair();
            let log_path = std::env::temp_dir().join(format!(
                "weaver-spu-null-refeed-{name}-{}.log",
                std::process::id()
            ));
            let log = std::fs::File::create(&log_path).expect("a child log file");
            let mut command = Command::new(env!("CARGO_BIN_EXE_weaver-spu"));
            command
                .stdin(Stdio::null())
                .stdout(Stdio::null())
                .stderr(Stdio::from(log));
            place_inherited(
                &mut command,
                &[child_lifecycle.as_raw_fd(), child_decode.as_raw_fd()],
            );
            let child = command.spawn().expect("the binary starts");
            bound_receives(&lifecycle, 120);
            bound_receives(&decode_parent, 120);
            let decode = weaver_spu::channel::decode_from_owned(decode_parent);
            let admitted = ask(
                &lifecycle,
                1,
                LifecycleDirective::Admit {
                    instruction: instruction(permission),
                },
            );
            assert_eq!(
                admitted.payload,
                Payload::Answer(LifecycleAnswer::Admitted),
                "the {name} residency admits"
            );
            (lifecycle, decode, child, log_path)
        };
        let close = |lifecycle,
                     decode,
                     mut child: std::process::Child,
                     log_path: std::path::PathBuf,
                     name: &str| {
            drop(decode);
            let released = ask(&lifecycle, 2, LifecycleDirective::Release);
            assert_eq!(released.payload, Payload::Answer(LifecycleAnswer::Released));
            drop(lifecycle);
            let status = wait_bounded(
                &mut child,
                30,
                &format!(
                    "the {name} worker exits, child stderr at {}",
                    log_path.display()
                ),
            );
            assert!(status.success(), "and the {name} process exits clean");
            std::fs::remove_file(&log_path).ok();
        };

        // **The source pass**: a serving-postured generation, recorded from
        // its own answer the way the trace records it.
        let (lifecycle, decode, child, log_path) = stand("source", false);
        let send = |decode: &weaver_spu::channel::DecodeSocket, directive: &TokenDirective| {
            let body = serde_json::to_vec(directive).expect("a directive renders");
            decode.send_octets(&body).expect("the frame sends");
        };
        let recv = |decode: &weaver_spu::channel::DecodeSocket| -> TokenAnswer {
            let frame = decode.recv_octets().expect("an answer arrives");
            serde_json::from_slice(&frame).expect("the answer parses")
        };
        send(
            &decode,
            &TokenDirective::Open {
                session: SessionId("s-null".into()),
                column_ask: false,
                messages: identity(),
            },
        );
        assert_eq!(
            recv(&decode),
            TokenAnswer::Opened,
            "the source session opens"
        );
        send(
            &decode,
            &TokenDirective::AppendAndGenerate {
                turn: TurnKey("t-1".into()),
                delta: vec![Message {
                    role: Role::User,
                    content: vec![ContentBlock::Text {
                        text: "Name any three colors.".into(),
                    }],
                }],
            },
        );
        let generation = loop {
            match recv(&decode) {
                TokenAnswer::Token { .. } => continue,
                TokenAnswer::Generated(generation) => break generation,
                other => panic!("the source stream carries tokens then the close: {other:?}"),
            }
        };
        let request: serde_json::Value =
            serde_json::from_str(generation.request.get()).expect("the request splices");
        let measurement: serde_json::Value =
            serde_json::from_str(generation.measurement.get()).expect("the measurement splices");
        let rendered = request["rendered"]
            .as_str()
            .expect("the rendered form")
            .to_string();
        let tokens = |value: &serde_json::Value| -> Vec<u32> {
            serde_json::from_value(value.clone()).expect("token identifiers")
        };
        let recorded_input = tokens(&measurement["input_tokens"]);
        let recorded_output = tokens(&measurement["output_tokens"]);
        assert!(
            !recorded_output.is_empty(),
            "the source generated something"
        );
        let recorded_emission = generation.emission.clone();
        let recorded_seed = request["sampling"]["generation_seed"].clone();
        close(lifecycle, decode, child, log_path, "source");

        // **The replay pass**: a fresh residency, the permission granted,
        // the registry's second arm read on the way in.
        let (lifecycle, decode, child, log_path) = stand("replay", true);
        send(
            &decode,
            &TokenDirective::Open {
                session: SessionId("s-null".into()),
                column_ask: false,
                messages: identity(),
            },
        );
        assert_eq!(
            recv(&decode),
            TokenAnswer::Opened,
            "the replay session opens"
        );
        send(
            &decode,
            &TokenDirective::ReFeed {
                turn: TurnKey("t-1".into()),
                rendered: rendered.clone(),
                path: vec![],
            },
        );
        let frame = decode.recv_octets().expect("the refusal arrives");
        let refusal: TokenRefusal = serde_json::from_slice(&frame).expect("the refusal parses");
        assert_eq!(
            refusal,
            TokenRefusal::RefeedPathEmpty,
            "a replay of nothing wearing an exchange refuses"
        );
        send(
            &decode,
            &TokenDirective::ReFeed {
                turn: TurnKey("t-1".into()),
                rendered,
                path: recorded_output.clone(),
            },
        );
        // **Under this instruction the re-feed answers in one frame, so the
        // first answer is the answer.** The drive streams no token of its
        // own, and the field and column intermediates cross only where their
        // elections stand, which this instruction sets off. So every arm
        // leaves on the first pass and a loop here read as though it might
        // iterate, which is what `never_loop` names. **An act that elects
        // either intermediate in this block re-opens the question** and wants
        // a drain with explicit arms, the way the siblings above skip tokens,
        // rather than this match.
        let refed = match recv(&decode) {
            TokenAnswer::ReFed(generation) => generation,
            TokenAnswer::Token { .. } => {
                panic!("the drive draws no token of its own to stream")
            }
            // Field and column frames reach here only if this instruction's
            // elections change, and the message says so rather than blaming
            // the answer's type.
            other => panic!("the re-feed answers in its own type under these elections: {other:?}"),
        };
        let refed_request: serde_json::Value =
            serde_json::from_str(refed.request.get()).expect("the re-fed request splices");
        let refed_measurement: serde_json::Value =
            serde_json::from_str(refed.measurement.get()).expect("the re-fed measurement splices");
        assert_eq!(
            tokens(&refed_measurement["input_tokens"]),
            recorded_input,
            "the rendered form re-tokenizes to the recorded appended input"
        );
        assert_eq!(
            tokens(&refed_measurement["output_tokens"]),
            recorded_output,
            "the recomputed draws equal the recorded path, integer for integer"
        );
        assert_eq!(
            refed.emission, recorded_emission,
            "the emission is the recorded path's text"
        );
        assert_eq!(
            refed_request["sampling"]["generation_seed"], recorded_seed,
            "the replay derived the seed the source drew from"
        );
        close(lifecycle, decode, child, log_path, "replay");
    }

    /// What this run's artifact owes per column: its layer count and the
    /// residual width, named by the operator where the artifact is not
    /// this file's fixture. Named rather than derived, on the same rule
    /// the readout's layer count follows.
    fn column_shape() -> (usize, usize) {
        // **Only an absent variable takes the fallback.** A named value that
        // does not parse is the operator's mistake, and reading it as the
        // fixture's shape would run the watch against a count belonging to
        // another artifact - which either fails far from its cause or, where
        // the shapes happen to agree, passes on a premise nobody stated. A
        // value that is not Unicode is named rather than absent and refuses
        // for that reason, `NotPresent` being the one arm that means nobody
        // asked.
        let named = |key: &str, fallback: usize| -> usize {
            match std::env::var(key) {
                Ok(value) => value.parse().unwrap_or_else(|_| {
                    panic!("{key} names a count, and this one does not parse: {value}")
                }),
                Err(std::env::VarError::NotPresent) => fallback,
                Err(other) => panic!("{key} is set and unreadable: {other}"),
            }
        };
        (
            named("WEAVER_ARTIFACT_COLUMN_LAYERS", 24),
            named("WEAVER_ARTIFACT_COLUMN_WIDTH", 896),
        )
    }

    /// **The column crosses where the ask stands, one message per sampled
    /// position.** The ask is judged at the open and armed for the
    /// residency, and the stream then carries a `Column` frame at each draw
    /// moment - the append's prefill final and each decode forward - with
    /// the layer count and the width the model's own, positions strictly
    /// increasing, tokens streaming beside them untouched.
    ///
    /// Perturbation: remove the take at the generate's draw site and this
    /// fails, the ask standing and no column crossing. Watched under
    /// exactly that removal.
    ///
    /// conforms: spu-column-crosses-where-asked
    #[test]
    fn the_column_crosses_where_the_ask_stands() {
        use weaver_traits::{ContentBlock, Message, Role};
        use weaver_types::{SessionId, TokenAnswer, TokenDirective, TurnKey};

        let Some(model) = model_present() else {
            eprintln!("SKIP the_column_crosses: no model at {MODEL}");
            return;
        };
        if device_context().is_none() {
            eprintln!("SKIP the_column_crosses: no CUDA device");
            return;
        }
        let _device = device_lock();

        let (lifecycle, child_lifecycle) = seqpacket_pair();
        let (decode_parent, child_decode) = seqpacket_pair();
        let log_path = std::env::temp_dir().join(format!(
            "weaver-spu-column-child-{}.log",
            std::process::id()
        ));
        let log = std::fs::File::create(&log_path).expect("a child log file");
        let mut command = Command::new(env!("CARGO_BIN_EXE_weaver-spu"));
        command
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::from(log));
        place_inherited(
            &mut command,
            &[child_lifecycle.as_raw_fd(), child_decode.as_raw_fd()],
        );
        let mut child = command.spawn().expect("the binary starts");
        bound_receives(&lifecycle, 120);
        bound_receives(&decode_parent, 120);
        let decode = weaver_spu::channel::decode_from_owned(decode_parent);

        let admitted = ask(
            &lifecycle,
            1,
            LifecycleDirective::Admit {
                instruction: SpuInstruction {
                    classify: None,
                    decoder: DecoderInstruction {
                        model_binding: ModelBinding {
                            artifact: ArtifactRef(model.to_string_lossy().into_owned()),
                            devices: vec![DeviceOrdinal(0)],
                        },
                        residual_readout_election: true,
                        field_election: None,
                        surprisal_election: false,
                        refeed_permission: false,
                        column_permission: true,
                        identity: vec![],
                        tunable_values: [
                            ("max-tokens-per-turn".to_string(), 24.0),
                            ("context-capacity".to_string(), 1024.0),
                            ("seed".to_string(), 37.0),
                        ]
                        .into_iter()
                        .collect(),
                    },
                },
            },
        );
        assert_eq!(admitted.payload, Payload::Answer(LifecycleAnswer::Admitted));

        let send = |directive: &TokenDirective| {
            let body = serde_json::to_vec(directive).expect("a directive renders");
            decode.send_octets(&body).expect("the frame sends");
        };
        let recv = || -> TokenAnswer {
            let frame = decode.recv_octets().expect("an answer arrives");
            serde_json::from_slice(&frame).expect("the answer parses")
        };

        send(&TokenDirective::Open {
            session: SessionId("s-column".into()),
            column_ask: true,
            messages: vec![Message {
                role: Role::User,
                content: vec![ContentBlock::Text {
                    text: "You answer in as few words as possible.".into(),
                }],
            }],
        });
        assert_eq!(recv(), TokenAnswer::Opened, "the asked open opens");

        send(&TokenDirective::AppendAndGenerate {
            turn: TurnKey("t-1".into()),
            delta: vec![Message {
                role: Role::User,
                content: vec![ContentBlock::Text {
                    text: "Say one word.".into(),
                }],
            }],
        });
        let mut columns: Vec<(u64, usize, usize)> = Vec::new();
        let mut streamed = 0usize;
        let generation = loop {
            match recv() {
                TokenAnswer::Token { .. } => streamed += 1,
                TokenAnswer::Column { position, layers } => {
                    let width = layers.first().map(Vec::len).unwrap_or(0);
                    assert!(
                        layers.iter().all(|l| l.len() == width),
                        "every layer at one width"
                    );
                    columns.push((position, layers.len(), width));
                }
                TokenAnswer::Generated(generation) => break generation,
                other => panic!("the stream carries tokens, columns, then the close: {other:?}"),
            }
        };
        assert!(streamed > 0, "the tokens still stream beside the columns");
        let measurement: serde_json::Value =
            serde_json::from_str(generation.measurement.get()).expect("the measurement splices");
        let produced = measurement["output_tokens"].as_array().unwrap().len();
        assert!(
            columns.len() == produced || columns.len() == produced + 1,
            "one column per sampled position: {} columns for {produced} tokens",
            columns.len()
        );
        // **The shape is the artifact's own and is named rather than
        // derived**, on the layer count's own rule: a shape taken from the
        // columns would agree with them whatever the tap did. The default
        // is this file's fixture and an operator pointing elsewhere names
        // what that artifact owes.
        let (want_layers, want_width) = column_shape();
        assert!(
            columns
                .iter()
                .all(|(_, layers, width)| *layers == want_layers && *width == want_width),
            "the model's own shape at every position, {want_layers}x{want_width}: {columns:?}"
        );
        assert!(
            columns.windows(2).all(|w| w[0].0 < w[1].0),
            "positions strictly increasing"
        );

        drop(decode);
        let released = ask(&lifecycle, 2, LifecycleDirective::Release);
        assert_eq!(released.payload, Payload::Answer(LifecycleAnswer::Released));
        drop(lifecycle);
        let status = wait_bounded(
            &mut child,
            30,
            &format!(
                "the column worker exits, child stderr at {}",
                log_path.display()
            ),
        );
        assert!(status.success(), "and the process exits clean");
        std::fs::remove_file(&log_path).ok();
    }

    /// **No vector crosses unasked, and the registry's first two arms
    /// refuse at the open.** One residency serves all three reads: an open
    /// carrying the ask against no permission refuses on the first arm, a
    /// permissionless posture being the serving one, and after a plain open
    /// the whole generation stream carries no `Column` frame.
    ///
    /// Perturbation for the absence: make the session's arming
    /// unconditional at the open and the plain open's stream carries
    /// columns. Watched under exactly that change.
    ///
    /// conforms: spu-no-vector-unasked
    #[test]
    fn no_vector_crosses_unasked_and_the_arms_refuse() {
        use weaver_traits::{ContentBlock, Message, Role};
        use weaver_types::{SessionId, TokenAnswer, TokenDirective, TokenRefusal, TurnKey};

        let Some(model) = model_present() else {
            eprintln!("SKIP no_vector_crosses_unasked: no model at {MODEL}");
            return;
        };
        if device_context().is_none() {
            eprintln!("SKIP no_vector_crosses_unasked: no CUDA device");
            return;
        }
        let _device = device_lock();

        let instruction = |readout: bool, permission: bool| SpuInstruction {
            classify: None,
            decoder: DecoderInstruction {
                model_binding: ModelBinding {
                    artifact: ArtifactRef(model.to_string_lossy().into_owned()),
                    devices: vec![DeviceOrdinal(0)],
                },
                residual_readout_election: readout,
                field_election: None,
                surprisal_election: false,
                refeed_permission: false,
                column_permission: permission,
                identity: vec![],
                tunable_values: [
                    ("max-tokens-per-turn".to_string(), 16.0),
                    ("context-capacity".to_string(), 1024.0),
                    ("seed".to_string(), 37.0),
                ]
                .into_iter()
                .collect(),
            },
        };

        // **Arm one, across the seam**: the ask against no permission.
        let (lifecycle, child_lifecycle) = seqpacket_pair();
        let (decode_parent, child_decode) = seqpacket_pair();
        let log_path = std::env::temp_dir().join(format!(
            "weaver-spu-unasked-child-{}.log",
            std::process::id()
        ));
        let log = std::fs::File::create(&log_path).expect("a child log file");
        let mut command = Command::new(env!("CARGO_BIN_EXE_weaver-spu"));
        command
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::from(log));
        place_inherited(
            &mut command,
            &[child_lifecycle.as_raw_fd(), child_decode.as_raw_fd()],
        );
        let mut child = command.spawn().expect("the binary starts");
        bound_receives(&lifecycle, 120);
        bound_receives(&decode_parent, 120);
        let decode = weaver_spu::channel::decode_from_owned(decode_parent);
        let admitted = ask(
            &lifecycle,
            1,
            LifecycleDirective::Admit {
                instruction: instruction(true, false),
            },
        );
        assert_eq!(admitted.payload, Payload::Answer(LifecycleAnswer::Admitted));

        let send = |directive: &TokenDirective| {
            let body = serde_json::to_vec(directive).expect("a directive renders");
            decode.send_octets(&body).expect("the frame sends");
        };
        let identity = || {
            vec![Message {
                role: Role::User,
                content: vec![ContentBlock::Text {
                    text: "You answer briefly.".into(),
                }],
            }]
        };
        send(&TokenDirective::Open {
            session: SessionId("s-unasked".into()),
            column_ask: true,
            messages: identity(),
        });
        let frame = decode.recv_octets().expect("the refusal arrives");
        let refusal: TokenRefusal = serde_json::from_slice(&frame).expect("the refusal parses");
        assert_eq!(
            refusal,
            TokenRefusal::ColumnPermissionAbsent,
            "the registry's first arm refuses across the seam"
        );

        // **The refused open opened nothing**: a plain open still stands,
        // and its whole stream carries no column.
        send(&TokenDirective::Open {
            session: SessionId("s-unasked".into()),
            column_ask: false,
            messages: identity(),
        });
        let recv = || -> TokenAnswer {
            let frame = decode.recv_octets().expect("an answer arrives");
            serde_json::from_slice(&frame).expect("the answer parses")
        };
        assert_eq!(recv(), TokenAnswer::Opened, "the plain open opens");
        send(&TokenDirective::AppendAndGenerate {
            turn: TurnKey("t-1".into()),
            delta: identity(),
        });
        loop {
            match recv() {
                TokenAnswer::Token { .. } => continue,
                TokenAnswer::Column { .. } => {
                    panic!("a vector crossed unasked")
                }
                TokenAnswer::Generated(_) => break,
                other => panic!("tokens then the close: {other:?}"),
            }
        }

        drop(decode);
        let released = ask(&lifecycle, 2, LifecycleDirective::Release);
        assert_eq!(released.payload, Payload::Answer(LifecycleAnswer::Released));
        drop(lifecycle);
        let status = wait_bounded(
            &mut child,
            30,
            &format!(
                "the unasked worker exits, child stderr at {}",
                log_path.display()
            ),
        );
        assert!(status.success(), "and the process exits clean");
        std::fs::remove_file(&log_path).ok();

        // **Arm two, across the seam**: permission admitted, readout
        // unelected.
        let (lifecycle, child_lifecycle) = seqpacket_pair();
        let (decode_parent, child_decode) = seqpacket_pair();
        let log_path =
            std::env::temp_dir().join(format!("weaver-spu-arm2-child-{}.log", std::process::id()));
        let log = std::fs::File::create(&log_path).expect("a child log file");
        let mut command = Command::new(env!("CARGO_BIN_EXE_weaver-spu"));
        command
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::from(log));
        place_inherited(
            &mut command,
            &[child_lifecycle.as_raw_fd(), child_decode.as_raw_fd()],
        );
        let mut child = command.spawn().expect("the binary starts");
        bound_receives(&lifecycle, 120);
        bound_receives(&decode_parent, 120);
        let decode = weaver_spu::channel::decode_from_owned(decode_parent);
        let admitted = ask(
            &lifecycle,
            1,
            LifecycleDirective::Admit {
                instruction: instruction(false, true),
            },
        );
        assert_eq!(admitted.payload, Payload::Answer(LifecycleAnswer::Admitted));
        let body = serde_json::to_vec(&TokenDirective::Open {
            session: SessionId("s-arm2".into()),
            column_ask: true,
            messages: identity(),
        })
        .expect("a directive renders");
        decode.send_octets(&body).expect("the frame sends");
        let frame = decode.recv_octets().expect("the refusal arrives");
        let refusal: TokenRefusal = serde_json::from_slice(&frame).expect("the refusal parses");
        assert_eq!(
            refusal,
            TokenRefusal::ColumnReadoutUnelected,
            "the registry's second arm refuses across the seam"
        );
        drop(decode);
        let released = ask(&lifecycle, 2, LifecycleDirective::Release);
        assert_eq!(released.payload, Payload::Answer(LifecycleAnswer::Released));
        drop(lifecycle);
        let status = wait_bounded(
            &mut child,
            30,
            &format!(
                "the arm-two worker exits, child stderr at {}",
                log_path.display()
            ),
        );
        assert!(status.success(), "and the process exits clean");
        std::fs::remove_file(&log_path).ok();
    }
}
