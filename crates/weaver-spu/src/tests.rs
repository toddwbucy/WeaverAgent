/// **The artifact refusal's account names the path and the identity**,
/// per `weaver-spu-Spec` section 3, and the cases the floor carries whole
/// leave no line.
///
/// Perturbation: drop `path` or `uid` from the line and this test fails.
/// Watched under exactly that removal.
#[test]
fn the_artifact_refusal_line_names_the_path_and_the_identity() {
    use weaver_spu::residency::AdmitRefusal;
    let refusal = AdmitRefusal::Unreadable {
        path: "/models/denied/model.gguf".into(),
        step: "resolve",
    };
    let line = super::admit_refusal_line(&refusal).expect("the artifact case has an account");
    let parsed: serde_json::Value = serde_json::from_str(&line).expect("one typed line");
    assert_eq!(parsed["refusal"], "artifact_unreadable");
    assert_eq!(parsed["path"], "/models/denied/model.gguf");
    assert_eq!(parsed["step"], "resolve");
    assert_eq!(parsed["uid"], nix::unistd::geteuid().as_raw());
    assert_eq!(parsed["gid"], nix::unistd::getegid().as_raw());
    let absent = AdmitRefusal::Unresolvable {
        path: "/models/absent.gguf".into(),
        step: "resolve",
    };
    let line = super::admit_refusal_line(&absent).expect("absent has an account too");
    assert!(line.contains("artifact_unresolvable") && line.contains("/models/absent.gguf"));
    assert!(
        super::admit_refusal_line(&AdmitRefusal::AlreadyAttempted).is_none(),
        "a case the floor carries whole leaves no line"
    );
}

use super::*;
use weaver_traits::{ContentBlock, Role};
use weaver_types::{
    AgentName, ArtifactRef, DecoderInstruction, DeviceOrdinal, ModelBinding, SpuInstruction,
};

/// **Every argument is read before the answer is given.**
///
/// Perturbation: return on the first `--headroom-bytes` rather than
/// recording it and continuing, and the trailing-unknown case passes with
/// the value served and the mistake unseen. Watched under exactly that.
#[test]
fn the_whole_vector_is_judged() {
    let of = |v: &[&str]| headroom_from(v.iter().map(|s| s.to_string()));
    assert_eq!(
        of(&[]),
        Ok(HEADROOM_BYTES),
        "nothing stated keeps the compiled default"
    );
    assert_eq!(of(&["--headroom-bytes", "1024"]), Ok(1024));
    assert!(
        of(&["--headroom-bytes", "1024", "--bogus"]).is_err(),
        "a misspelled parameter after a good one is still a mistake"
    );
    assert!(
        of(&["--headroom-bytes", "1024", "--headroom-bytes", "2048"]).is_err(),
        "stated twice picks for a deployment that meant one of them"
    );
    assert!(
        of(&["--headroom-bytes"]).is_err(),
        "the value is not optional"
    );
    assert!(
        of(&["--headroom-bytes", "many"]).is_err(),
        "and it is a byte count"
    );
}

fn directive(directive: LifecycleDirective) -> OrganEnvelope {
    OrganEnvelope {
        exchange: ExchangeId {
            opener: Opener::Harness,
            ordinal: 1,
        },
        position: Position::Open,
        payload: Payload::Directive(directive),
    }
}

fn binding() -> ModelBinding {
    ModelBinding {
        artifact: ArtifactRef("/nonexistent/artifact".into()),
        devices: vec![DeviceOrdinal(0)],
    }
}

fn instruction() -> SpuInstruction {
    SpuInstruction {
        classify: None,
        decoder: DecoderInstruction {
            model_binding: binding(),
            residual_readout_election: false,
            field_election: None,
            surprisal_election: false,
            refeed_permission: false,
            column_permission: false,
            tunable_values: [
                ("max-tokens-per-turn".to_string(), 4096.0),
                ("context-capacity".to_string(), 4096.0),
                // The seat's own draw, tunable as of 2026-08-20.
                ("seed".to_string(), 11.0),
            ]
            .into_iter()
            .collect(),
        },
    }
}

/// **A release before any admit answers `OutOfOrder`,** per Spec sections 9
/// and 10: the order is judged against the seam's recorded position before
/// the directive reaches residency, so the refusal is the position's and
/// not residency's `NoResidency`.
///
/// Perturbation: route `(BeforeAdmit, Release)` through
/// `residency.release()` and this test fails, because the answer becomes
/// `NoResidency`. Watched under exactly that change.
#[test]
fn a_release_before_any_admit_answers_out_of_order() {
    let mut residency = Residency::new();
    let mut position = SeamPosition::BeforeAdmit;
    assert_eq!(
        dispatch(
            &mut position,
            &mut residency,
            &directive(LifecycleDirective::Release),
            &mut None,
            HEADROOM_BYTES,
        ),
        Payload::Refusal(LifecycleRefusal::OutOfOrder)
    );
    assert_eq!(position, SeamPosition::BeforeAdmit, "and is not queued");
}

/// **The released position is terminal.** A directive of any kind arriving
/// after a release answers `OutOfOrder`. The seam cannot reach this
/// position today, no build being able to admit, so the machine is driven
/// to it directly: the state is this crate's own and the test reads the
/// judgment, not the journey.
#[test]
fn any_directive_after_a_release_answers_out_of_order() {
    let mut residency = Residency::new();
    let mut position = SeamPosition::Released;
    for case in [
        LifecycleDirective::Admit {
            instruction: instruction(),
        },
        LifecycleDirective::Release,
        LifecycleDirective::Stop {
            cause: weaver_types::Cause { uid: 0 },
        },
    ] {
        assert_eq!(
            dispatch(
                &mut position,
                &mut residency,
                &directive(case.clone()),
                &mut None,
                HEADROOM_BYTES
            ),
            Payload::Refusal(LifecycleRefusal::OutOfOrder),
            "{case:?} after a release is out of order"
        );
        assert_eq!(position, SeamPosition::Released, "terminal stays terminal");
    }
}

/// **A second admit answers `OutOfOrder` whatever the first answered.** The
/// first spends the one admission even when it refuses, and the second is
/// refused at the position rather than reaching residency's resolution
/// step.
#[test]
fn a_second_admit_is_refused_at_the_position() {
    let mut residency = Residency::new();
    let mut position = SeamPosition::BeforeAdmit;

    let first = dispatch(
        &mut position,
        &mut residency,
        &directive(LifecycleDirective::Admit {
            instruction: instruction(),
        }),
        &mut None,
        HEADROOM_BYTES,
    );
    assert_eq!(
        first,
        Payload::Refusal(LifecycleRefusal::ArtifactUnresolvable),
        "the first admit runs and refuses on the artifact"
    );
    assert_eq!(position, SeamPosition::AdmitRefused);

    let second = dispatch(
        &mut position,
        &mut residency,
        &directive(LifecycleDirective::Admit {
            instruction: instruction(),
        }),
        &mut None,
        HEADROOM_BYTES,
    );
    assert_eq!(second, Payload::Refusal(LifecycleRefusal::OutOfOrder));
}

/// **Only admit and release cross this seam.** Every other case of the
/// floor's closed set refuses as out of order, at every position, the
/// position unchanged (#99 area 3, S01): an admitted SPU that answered a
/// `SavePoint`, a `ForceUnload` or a `Restore` would claim a part of a
/// protocol the contract never gave it. `Enter` is the harness's whole
/// instruction and is refused by the same arm.
///
/// Perturbation: answer `Stop` with `AtRest` in `dispatch` and this test
/// fails on the `Stop` case; answer `SavePoint` or `ForceUnload` while
/// admitted and it fails on that case.
#[test]
fn a_directive_outside_the_drawn_vocabulary_refuses_out_of_order() {
    let cause = weaver_types::Cause { uid: 0 };
    let agent = || AgentName("alpha".into());
    let outside = || {
        vec![
            LifecycleDirective::Leave {
                cause,
                forced: false,
                rollback: false,
            },
            LifecycleDirective::Leave {
                cause,
                forced: true,
                rollback: false,
            },
            LifecycleDirective::JoinLeave { cause },
            LifecycleDirective::Quiesce,
            LifecycleDirective::SavePoint { cause },
            LifecycleDirective::Stop { cause },
            LifecycleDirective::Observe,
            LifecycleDirective::Raise {
                instruction: weaver_types::GateInstruction {
                    access_rule: weaver_types::AccessRule {
                        allowed_uids: Default::default(),
                        allowed_gids: Default::default(),
                        denied_uids: Default::default(),
                    },
                },
                socket: "/nonexistent/gate.sock".into(),
            },
            LifecycleDirective::Lower,
            LifecycleDirective::Load { agent: agent() },
            LifecycleDirective::Unload { agent: agent() },
            LifecycleDirective::Validate { agent: agent() },
            LifecycleDirective::Show { agent: agent() },
            LifecycleDirective::SavePointVerb { agent: agent() },
            LifecycleDirective::Restore { agent: agent() },
            LifecycleDirective::ForceUnload { agent: agent() },
        ]
    };
    for (start, name) in [
        (SeamPosition::BeforeAdmit, "before the admit"),
        (SeamPosition::AdmitRefused, "after a refused admit"),
        (SeamPosition::Admitted, "while admitted"),
        (SeamPosition::Released, "after the release"),
    ] {
        let mut position = start;
        let mut residency = Residency::new();
        for case in outside() {
            assert_eq!(
                dispatch(
                    &mut position,
                    &mut residency,
                    &directive(case.clone()),
                    &mut None,
                    HEADROOM_BYTES
                ),
                Payload::Refusal(LifecycleRefusal::OutOfOrder),
                "{case:?} {name} is outside this seam's vocabulary"
            );
            assert_eq!(position, start, "{case:?} {name}: not queued");
        }
    }
}

/// A directive that does not open its exchange, or an exchange claiming an
/// opener that is not the harness, is the peer speaking out of turn, judged
/// before the directive's own case is looked at.
///
/// Perturbation: remove the position-and-opener check at the top of
/// `dispatch` and this test fails, because the well-formed admit inside the
/// mis-shapen envelope executes and spends the one admission. Watched under
/// exactly that removal.
#[test]
fn a_mis_shapen_exchange_refuses_before_the_directive_runs() {
    let mut residency = Residency::new();
    let mut position = SeamPosition::BeforeAdmit;

    let mut closing = directive(LifecycleDirective::Admit {
        instruction: instruction(),
    });
    closing.position = Position::Close;
    assert_eq!(
        dispatch(
            &mut position,
            &mut residency,
            &closing,
            &mut None,
            HEADROOM_BYTES
        ),
        Payload::Refusal(LifecycleRefusal::OutOfOrder)
    );

    let mut wrong_opener = directive(LifecycleDirective::Admit {
        instruction: instruction(),
    });
    wrong_opener.exchange.opener = Opener::Spu;
    assert_eq!(
        dispatch(
            &mut position,
            &mut residency,
            &wrong_opener,
            &mut None,
            HEADROOM_BYTES
        ),
        Payload::Refusal(LifecycleRefusal::OutOfOrder)
    );

    assert_eq!(
        position,
        SeamPosition::BeforeAdmit,
        "neither mis-shapen envelope spent the admission"
    );
}

/// A payload that is not a directive is the peer speaking out of turn.
#[test]
fn a_non_directive_payload_refuses_out_of_order() {
    let mut residency = Residency::new();
    let mut position = SeamPosition::BeforeAdmit;
    let envelope = OrganEnvelope {
        exchange: ExchangeId {
            opener: Opener::Harness,
            ordinal: 1,
        },
        position: Position::Open,
        payload: Payload::Answer(LifecycleAnswer::Ready),
    };
    assert_eq!(
        dispatch(
            &mut position,
            &mut residency,
            &envelope,
            &mut None,
            HEADROOM_BYTES
        ),
        Payload::Refusal(LifecycleRefusal::OutOfOrder)
    );
}

fn open() -> TokenDirective {
    TokenDirective::Open {
        session: weaver_types::SessionId("s-1".into()),
        messages: vec![],
        column_ask: false,
    }
}

fn append() -> TokenDirective {
    TokenDirective::AppendAndGenerate {
        turn: weaver_types::TurnKey("t-1".into()),
        delta: vec![],
    }
}

/// **An open before the residency it serves is confirmed refuses and is
/// not queued,** per Spec section 9, one contract's ordering read against
/// the other's. The judgment is pure and precedes the session, so the
/// refusal is about the order and never about work already run.
///
/// Perturbation: make `judge_decode` hold an early open for later and
/// this test fails, there being no refusal to read. Watched under exactly
/// that change.
#[test]
fn an_open_before_residency_refuses_and_after_it_is_in_order() {
    assert_eq!(
        judge_decode(DecodePosition::BeforeOpen, false, &open()),
        Some(TokenRefusal::OutOfOrder),
        "before residency, out of order"
    );
    assert_eq!(
        judge_decode(DecodePosition::BeforeOpen, true, &open()),
        None,
        "after residency, in order"
    );
}

/// **A directive before any open answers `NotOpen`,** the refusal case
/// `weaver-types-Spec` section 4.4 gives the not-open session, distinct
/// from the ordering refusal because the harness reads the two
/// differently: one says open first, the other says never.
#[test]
fn a_directive_before_any_open_answers_not_open() {
    for directive in [
        append(),
        TokenDirective::Cancel {
            turn: weaver_types::TurnKey("t-1".into()),
        },
        TokenDirective::Flush { keep: 0 },
    ] {
        assert_eq!(
            judge_decode(DecodePosition::BeforeOpen, true, &directive),
            Some(TokenRefusal::NotOpen),
            "{directive:?} before any open"
        );
    }
}

/// **A second open refuses rather than rewinding,** per the contract's
/// open-first-and-once, the flush being what a rewind has instead.
#[test]
fn a_second_open_refuses_rather_than_rewinding_on_the_seam() {
    assert_eq!(
        judge_decode(DecodePosition::AtRest, true, &open()),
        Some(TokenRefusal::OutOfOrder)
    );
}

/// **A block the family cannot render refuses as the delta malformed for
/// the family,** the contract's own case. The tool workflow lifted the
/// blanket block, so the subject here is a shape no party authors: a
/// tool call riding a user turn, which qwen2 renders for the assistant
/// alone.
///
/// What this reads is the refusal arriving at the seam's own vocabulary,
/// so it exercises the family's render and the [`From`] that carries its
/// answer across. The role arm's wildcard is the future's alone and no
/// test can construct its subject, `Role` being non-exhaustive with every
/// current case rendered by the families that call the shared name kernel.
#[test]
fn a_non_text_block_refuses_as_malformed_delta() {
    let message = Message {
        role: Role::User,
        content: vec![ContentBlock::ToolCall(weaver_traits::ToolCall {
            name: "calculator".into(),
            arguments: "{}".into(),
        })],
    };
    assert_eq!(
        render_delta(family::qwen2::renderer(), &[message]),
        Err(TokenRefusal::MalformedDelta),
    );
}

/// **A tool result is not renderable under gemma4, and the refusal is the
/// family's rather than the root's.**
///
/// The family has no tool turn: its template carries tool results as
/// standalone blocks outside the turn structure. Rendering one as
/// `<|turn>tool` would be a silent substitution, so it refuses until the
/// tool workflow names the block shape.
///
/// Perturbation: give `Gemma4::role_name` a `Role::ToolResult => "tool"`
/// arm and this fails, the render then succeeding on a turn shape the
/// artifact's template never produces.
#[test]
fn gemma4_refuses_a_tool_result_rather_than_inventing_a_turn() {
    let message = Message {
        role: Role::ToolResult,
        content: vec![ContentBlock::Text {
            text: "42".to_string(),
        }],
    };
    assert_eq!(
        render_delta(family::gemma4::renderer(), &[message]),
        Err(TokenRefusal::MalformedDelta),
    );

    // The contrast, so this reads as a distinction rather than as a family
    // that refuses everything: the authored shape - the granted door's
    // result block in the tool-result role - renders under qwen2.
    let message = Message {
        role: Role::ToolResult,
        content: vec![ContentBlock::ToolResult(weaver_traits::ToolResultBlock {
            content: "42".to_string(),
        })],
    };
    assert!(render_delta(family::qwen2::renderer(), &[message]).is_ok());
}

/// At rest, a cancel and a flush are both in order for the seam: the
/// cancel answers at rest rather than refusing, which is the executor's
/// answer and not this judgment's refusal.
#[test]
fn cancel_and_flush_are_in_order_at_rest() {
    assert_eq!(
        judge_decode(
            DecodePosition::AtRest,
            true,
            &TokenDirective::Cancel {
                turn: weaver_types::TurnKey("t-1".into()),
            },
        ),
        None
    );
    assert_eq!(
        judge_decode(
            DecodePosition::AtRest,
            true,
            &TokenDirective::Flush { keep: 0 }
        ),
        None
    );
}
