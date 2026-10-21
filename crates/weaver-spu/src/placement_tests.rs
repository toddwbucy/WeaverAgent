//! The host watches preparation ordering without fabricating a Resident. The
//! device instrument below uses the existing owner and actual residency for
//! measurement, cuts and generation on both backend paths.

use super::placement::{PlacementFailure, prepare_generation};
use weaver_spu::decoder::backend::{DecodeFault, TokenId};
use weaver_spu::family;
use weaver_traits::{ContentBlock, Message, Role};
use weaver_types::TokenRefusal;

fn text(role: Role, body: &str) -> Message {
    Message {
        role,
        content: vec![ContentBlock::Text { text: body.into() }],
    }
}

fn malformed() -> Message {
    Message {
        role: Role::User,
        content: vec![ContentBlock::ToolCall(weaver_traits::ToolCall {
            name: "calculator".into(),
            arguments: "{}".into(),
        })],
    }
}

#[test]
fn preparation_tokenizes_one_complete_render_including_the_boundary() {
    let renderer = family::qwen2::renderer();
    let delta = [text(Role::User, "Read the file.")];
    let schemas = ["schema one".into(), "schema two".into()];
    let opener = "<assistant>";
    let expected = family::render_generation(renderer, &delta, &schemas, opener).unwrap();
    // A tiny non-additive vocabulary: newline followed by '<' is one token.
    // Splitting off the opener would count that boundary twice.
    let tokenize = |input: &str| {
        let count = input.len() - input.match_indices("\n<").count();
        vec![TokenId(7); count]
    };
    let separately =
        tokenize(expected.strip_suffix(opener).unwrap()).len() + tokenize(opener).len();
    let whole = tokenize(&expected);
    assert_ne!(
        separately,
        whole.len(),
        "the boundary fixture is non-additive"
    );
    let mut calls = 0;
    let prepared = prepare_generation(renderer, opener, &delta, &schemas, |input| {
        calls += 1;
        assert_eq!(input, expected);
        Ok(tokenize(input))
    })
    .unwrap();
    assert_eq!(calls, 1);
    assert_eq!(prepared.rendered, expected);
    assert_eq!(prepared.tokens, whole);
}

#[test]
fn malformed_render_never_calls_the_tokenizer() {
    let result = prepare_generation(
        family::qwen2::renderer(),
        "<assistant>",
        &[malformed()],
        &["schema".into()],
        |_| panic!("a malformed delta reached tokenization"),
    );
    assert_eq!(
        result,
        Err(PlacementFailure::Render(TokenRefusal::MalformedDelta))
    );
}

#[test]
fn tokenizer_failure_remains_a_component_fault_and_empty_input_keeps_the_opener() {
    let fault = DecodeFault::Engine {
        detail: "tokenizer failed".into(),
    };
    let result = prepare_generation(
        family::qwen2::renderer(),
        "<assistant>",
        &[],
        &[],
        |input| {
            assert_eq!(input, "<assistant>");
            Err(fault.clone())
        },
    );
    assert_eq!(result, Err(PlacementFailure::Decode(fault)));
}

#[cfg(feature = "cuda")]
mod device {
    use super::*;
    use crate::placement::{DeltaMeasured, OpenFacts, OpenedSession};
    use std::path::{Path, PathBuf};
    use weaver_spu::decoder::session::{
        CancelPoll, Generated, NeverCancels, PositionedSinks, SamplerBuild, StopCondition, Stopped,
    };
    use weaver_spu::readout::ReadoutElection;
    use weaver_spu::residency::{Headroom, Residency, Resident};
    use weaver_spu::sampling::{
        Disposition, EffectiveKnobs, EffectiveSessionParameters, SessionParameters, TunableValues,
    };
    use weaver_types::{ArtifactRef, DeviceOrdinal, ModelBinding, TurnKey};

    const CHILD: &str = "WEAVER_PLACEMENT_TEST_BACKEND";

    fn artifact(kind: &str) -> Option<PathBuf> {
        let (variable, fallback, directory) = match kind {
            "gguf" => (
                "WEAVER_TEST_GGUF",
                "/opt/weaver/models/qwen2.5-0.5b-instruct-q6_k.gguf",
                false,
            ),
            "native" => (
                "WEAVER_ARTIFACT_QWEN25_SAFETENSORS",
                "/bulk-store/models/Qwen--Qwen2.5-0.5B-Instruct",
                true,
            ),
            _ => panic!("unknown placement fixture backend"),
        };
        let present = |path: &Path| {
            if directory {
                path.is_dir()
            } else {
                path.is_file()
            }
        };
        match std::env::var_os(variable) {
            Some(named) => {
                let path = PathBuf::from(named);
                assert!(
                    present(&path),
                    "{variable} names an unavailable artifact: {}",
                    path.display()
                );
                Some(path)
            }
            None => present(Path::new(fallback)).then(|| PathBuf::from(fallback)),
        }
    }

    /// One admit per process, matching the serving lifecycle and avoiding the
    /// engine's known free-then-reload hazard. The wrapper serializes backends.
    #[test]
    fn real_residencies_measure_cut_and_generate_without_observable_mutation() {
        let kinds = if cfg!(feature = "gguf") {
            &["gguf", "native"][..]
        } else {
            &["native"][..]
        };
        for kind in kinds {
            if artifact(kind).is_none() {
                eprintln!("SKIP placement {kind}: no implicit fixture artifact");
                continue;
            }
            if cudarc::driver::CudaContext::new(0).is_err() {
                let variable = if *kind == "gguf" {
                    "WEAVER_TEST_GGUF"
                } else {
                    "WEAVER_ARTIFACT_QWEN25_SAFETENSORS"
                };
                assert!(
                    std::env::var_os(variable).is_none(),
                    "{variable} explicitly elected an artifact but no CUDA device is available"
                );
                eprintln!("SKIP placement {kind}: no CUDA device");
                continue;
            }
            let result = std::process::Command::new(std::env::current_exe().unwrap())
                .args([
                    "--exact",
                    "placement_tests::device::real_residency_child",
                    "--nocapture",
                    "--test-threads=1",
                ])
                .env(CHILD, kind)
                .output()
                .expect("start one residency process");
            assert!(
                result.status.success(),
                "placement {kind} failed: {}\n{}",
                String::from_utf8_lossy(&result.stdout),
                String::from_utf8_lossy(&result.stderr),
            );
            eprintln!(
                "placement {kind}: real residency, shared preparation, cuts and generation passed"
            );
        }
    }

    fn knobs() -> EffectiveKnobs {
        EffectiveKnobs {
            temperature: 0.7,
            top_k: 40,
            top_p: 0.95,
            repetition_penalty: 1.1,
            repetition_window: 32,
            seed: 11,
        }
    }

    fn open(resident: &Resident, parameters: EffectiveSessionParameters) -> OpenedSession<'_> {
        let renderer = (resident.declaration.renderer)();
        let prefix = renderer
            .render_identity(&[text(Role::System, "Answer briefly.")])
            .unwrap();
        let tokens = resident.tokenize(&prefix).unwrap();
        let mut session = resident
            .open_session(&knobs(), parameters.context_capacity)
            .unwrap();
        session.open(&tokens).unwrap();
        OpenedSession::from_open(
            session,
            resident,
            parameters,
            resident.stop_set(renderer).unwrap(),
        )
    }

    fn generate(
        standing: &mut OpenedSession<'_>,
        input: &[TokenId],
        limit: usize,
        cancel: &mut dyn CancelPoll,
        seed: u64,
    ) -> Result<Generated, DecodeFault> {
        standing.session.append_and_generate(
            input,
            &StopCondition {
                stop_tokens: standing.stop.tokens.clone(),
                terminator: standing.stop.terminator,
                max_tokens: limit,
            },
            cancel,
            &mut |_| {},
            PositionedSinks {
                field: None,
                on_column: &mut |_, _| {},
            },
            SamplerBuild {
                seed,
                penalty_window: 32,
            },
        )
    }

    fn probe(standing: &OpenedSession<'_>, delta: &[Message], schemas: &[String]) -> DeltaMeasured {
        let position = standing.session.placement_position(u64::MAX).unwrap();
        let stop = standing.stop.clone();
        let ordinal = standing.turn_generations.clone();
        let lower = standing.measure_delta(0, delta, schemas).unwrap();
        let upper = standing.measure_delta(u64::MAX, delta, schemas).unwrap();
        assert_eq!(lower.keep, position.prefix_tokens);
        assert_eq!(upper.keep, position.resident);
        assert_eq!(upper.resident, position.resident);
        assert_eq!(upper.capacity, position.capacity);
        assert_eq!(upper.prefix_tokens, position.prefix_tokens);
        assert_eq!(upper.input_tokens, lower.input_tokens);
        assert_eq!(
            standing.measure_delta(0, &[malformed()], schemas),
            Err(PlacementFailure::Render(TokenRefusal::MalformedDelta))
        );
        // A syntactically renderable proposal is measured even when it cannot
        // fit. It is not truncated to capacity and no zero substitutes for it.
        let huge = [text(Role::User, &"repeat this long proposal ".repeat(1024))];
        let oversized = standing.measure_delta(0, &huge, schemas).unwrap();
        assert!(oversized.input_tokens > oversized.capacity);
        assert_eq!(
            standing.session.placement_position(u64::MAX).unwrap(),
            position
        );
        assert_eq!(standing.stop, stop);
        assert_eq!(standing.turn_generations, ordinal);
        upper
    }

    /// Compare an unmeasured run with the same run carrying repeated valid,
    /// malformed and oversized queries. Timings are intentionally excluded;
    /// sampled tokens, stop outcomes, entropy/surprisal and residual figures
    /// must agree after ordinary generation, both cuts and a cancellation.
    fn sequence(
        resident: &Resident,
        parameters: EffectiveSessionParameters,
        measure: bool,
    ) -> Vec<Generated> {
        let mut standing = open(resident, parameters);
        standing.turn_generations = Some((TurnKey("placement turn".into()), 7));
        let facts = standing.open_facts().unwrap();
        assert_eq!(
            facts,
            OpenFacts {
                prefix_tokens: standing.session.prefix_len() as u64,
                capacity: u64::from(parameters.context_capacity),
                generation_limit: parameters.max_tokens_per_turn as u64,
                terminal_tokens: 1,
            }
        );
        let delta = [text(Role::User, "Count from one to ten.")];
        let schemas = [
            "Tool: read a file.".into(),
            "Tool: list a directory.".into(),
        ];
        let mut generations = Vec::new();
        for step in 0..5 {
            // The first pass has exactly the legacy no-schema shape; later
            // passes include the complete schema rendering.
            let schema_input = if step == 0 { &[][..] } else { &schemas[..] };
            let measured = measure.then(|| probe(&standing, &delta, schema_input));
            if step == 1 {
                // The prefix clamp returned by a hypothetical cut is the
                // actual Flush bound; tokenization is independent of that cut.
                let keep = if measure {
                    standing
                        .measure_delta(0, &delta, schema_input)
                        .unwrap()
                        .keep
                } else {
                    facts.prefix_tokens
                };
                standing.session.flush(keep.try_into().unwrap()).unwrap();
            }
            if step == 2 {
                let before = standing.session.resident_len();
                let from = standing.session.prefix_len() + 1;
                assert!(from < before, "the suffix fixture removes resident tokens");
                let (old, kept) = standing.session.elide(from, before).unwrap();
                assert_eq!(old, before);
                assert_eq!(kept, from);
            }
            let prepared = standing.prepare_delta(&delta, schema_input).unwrap();
            if let Some(measured) = measured {
                assert_eq!(measured.input_tokens, prepared.tokens.len() as u64);
                let after_cut = standing
                    .measure_delta(u64::MAX, &delta, schema_input)
                    .unwrap();
                assert_eq!(after_cut.input_tokens, measured.input_tokens);
                assert_eq!(after_cut.resident, standing.session.resident_len() as u64);
            }
            let before = standing.session.resident_len();
            struct Cancel;
            impl CancelPoll for Cancel {
                fn cancelled(&mut self) -> bool {
                    true
                }
            }
            let mut cancel = Cancel;
            let mut never = NeverCancels;
            let poll: &mut dyn CancelPoll = if step == 3 { &mut cancel } else { &mut never };
            let mut generated = generate(
                &mut standing,
                &prepared.tokens,
                parameters.max_tokens_per_turn,
                poll,
                101 + step,
            )
            .unwrap();
            assert_eq!(
                standing.session.resident_len(),
                before + prepared.tokens.len() + generated.tokens.len() + 1
            );
            if step == 3 {
                assert_eq!(generated.stopped, Stopped::Cancelled);
                assert!(generated.tokens.is_empty());
            } else {
                assert!(
                    !generated.tokens.is_empty(),
                    "ordinary generation must exercise the sampler"
                );
                assert_ne!(generated.stopped, Stopped::Cancelled);
                assert!(generated.signals.entropy_bits.is_some());
                assert!(generated.signals.surprisal_bits.is_some());
            }
            assert!(
                generated.residual.is_some(),
                "elected readout still reports this generation"
            );
            let report: serde_json::Value = serde_json::from_str(&crate::render_measurement(
                resident,
                &prepared.tokens,
                &generated,
                prepared.rendered.len(),
            ))
            .unwrap();
            assert_eq!(
                report["input_tokens"].as_array().unwrap().len(),
                prepared.tokens.len()
            );
            assert_eq!(
                report["output_tokens"].as_array().unwrap().len(),
                generated.tokens.len()
            );
            generated.prefill_ns = 0;
            generated.decode_ns = 0;
            generations.push(generated);
        }
        if measure {
            let huge = standing
                .prepare_delta(&[text(Role::User, &"proposal ".repeat(2048))], &[])
                .unwrap();
            let before = standing.session.placement_position(0).unwrap();
            assert!(matches!(
                generate(&mut standing, &huge.tokens, 4, &mut NeverCancels, 900),
                Err(DecodeFault::Overflow { .. })
            ));
            assert_eq!(standing.session.placement_position(0).unwrap(), before);
        }
        standing.session.close();
        assert_eq!(standing.open_facts(), Err(DecodeFault::NotOpen));
        assert_eq!(
            standing.measure_delta(0, &[malformed()], &[]),
            Err(PlacementFailure::Decode(DecodeFault::NotOpen))
        );
        generations
    }

    /// Exercise the actual composition-root branch over its legacy socket.
    /// Measurement's proposal is not a reservation: this separate open repeats
    /// preparation and must emit those exact input ids in the existing report.
    fn legacy_decode_agrees(resident: &Resident, parameters: EffectiveSessionParameters) {
        use nix::sys::socket::{AddressFamily, SockFlag, SockType, setsockopt, socketpair};
        use nix::sys::time::TimeVal;
        use weaver_spu::channel::decode_from_owned;
        use weaver_types::{SessionId, TokenAnswer, TokenDirective};

        let delta = vec![text(Role::User, "Count from one to ten.")];
        let (measured, prepared, prefix) = {
            let mut observer = open(resident, parameters);
            let measured = observer.measure_delta(u64::MAX, &delta, &[]).unwrap();
            let prepared = observer.prepare_delta(&delta, &[]).unwrap();
            let prefix = observer.open_facts().unwrap().prefix_tokens;
            observer.session.close();
            (measured, prepared, prefix)
        };
        let (client, server) = socketpair(
            AddressFamily::Unix,
            SockType::SeqPacket,
            None,
            SockFlag::SOCK_CLOEXEC,
        )
        .unwrap();
        for end in [&client, &server] {
            setsockopt(
                end,
                nix::sys::socket::sockopt::ReceiveTimeout,
                &TimeVal::new(60, 0),
            )
            .unwrap();
        }
        let client = std::thread::spawn(move || {
            let channel = decode_from_owned(client);
            let send = |directive: TokenDirective| {
                channel
                    .send_octets(&serde_json::to_vec(&directive).unwrap())
                    .unwrap();
            };
            send(TokenDirective::Open {
                session: SessionId("placement legacy seam".into()),
                messages: vec![text(Role::System, "Answer briefly.")],
                column_ask: false,
            });
            let opened = channel.recv_octets().unwrap();
            assert_eq!(
                serde_json::from_slice::<serde_json::Value>(&opened).unwrap(),
                serde_json::json!({"kind": "opened"})
            );
            send(TokenDirective::AppendAndGenerate {
                turn: TurnKey("placement seam turn".into()),
                delta,
            });
            let generated = loop {
                match serde_json::from_slice::<TokenAnswer>(&channel.recv_octets().unwrap())
                    .unwrap()
                {
                    TokenAnswer::Token { .. } => {}
                    TokenAnswer::Generated(generated) => break generated,
                    other => panic!("unexpected legacy decode answer: {other:?}"),
                }
            };
            let measurement: serde_json::Value =
                serde_json::from_str(generated.measurement.get()).unwrap();
            let request: serde_json::Value = serde_json::from_str(generated.request.get()).unwrap();
            assert_eq!(
                measurement["input_tokens"],
                serde_json::json!(
                    prepared
                        .tokens
                        .iter()
                        .map(|token| token.0)
                        .collect::<Vec<_>>()
                )
            );
            assert_eq!(
                measurement["input_tokens"].as_array().unwrap().len() as u64,
                measured.input_tokens
            );
            assert_eq!(request["rendered"], prepared.rendered);
            assert_eq!(
                generated.resident,
                prefix
                    + measured.input_tokens
                    + measurement["output_tokens"].as_array().unwrap().len() as u64
                    + 1
            );
            assert_eq!(generated.capacity, measured.capacity);
        });
        let result = crate::serve_decode(
            &decode_from_owned(server),
            resident,
            &crate::Effective {
                knobs: knobs(),
                session: parameters,
                field_depth: None,
                refeed_permission: false,
                column_permission: false,
                readout_elected: true,
            },
        );
        client
            .join()
            .expect("the legacy peer completes its exchange");
        assert_eq!(result, Ok(()));
    }

    #[test]
    fn real_residency_child() {
        let Ok(kind) = std::env::var(CHILD) else {
            return;
        };
        let path = artifact(&kind).expect("wrapper verified model availability");
        let mut residency = Residency::new();
        let resident = residency
            .admit(
                &ModelBinding {
                    artifact: ArtifactRef(path.to_string_lossy().into_owned()),
                    devices: vec![DeviceOrdinal(0)],
                },
                Headroom(64 * 1024 * 1024),
                ReadoutElection(true),
                true,
            )
            .expect("real model admits with readout and surprisal");
        let parameters = SessionParameters {
            context_capacity: Disposition::Frozen(512),
            max_tokens_per_turn: Disposition::OperatorTunable,
        }
        .resolve(&TunableValues::from([("max-tokens-per-turn".into(), 4.0)]))
        .unwrap();
        let baseline = sequence(resident, parameters, false);
        let measured = sequence(resident, parameters, true);
        assert_eq!(
            measured, baseline,
            "read-only measurement changes no generation or readout"
        );
        let frozen = SessionParameters {
            context_capacity: Disposition::Frozen(512),
            max_tokens_per_turn: Disposition::Frozen(3),
        }
        .resolve(&TunableValues::new())
        .unwrap();
        let mut standing = open(resident, frozen);
        assert_eq!(standing.open_facts().unwrap().generation_limit, 3);
        standing.session.close();
        drop(standing);
        legacy_decode_agrees(resident, parameters);
    }
}
