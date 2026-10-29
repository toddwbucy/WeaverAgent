//! Scripted peers for dispatcher input, save and graceful wind-down cases.
//! The ordinary startup and release owners still drive the real child sockets.
use super::*;

pub(super) fn two_turns(mode: &str) -> bool {
    mode.contains("-session-graceful-save-dispatch-") && mode.ends_with("-two")
}

pub(super) fn pending_force(mode: &str) -> bool {
    mode.contains("-session-forced-save-dispatch-") && mode.ends_with("-pending")
}

pub(super) fn resting(mode: &str) -> bool {
    mode.contains("-session-graceful-save-dispatch-") && mode.ends_with("-s9")
}

pub(super) fn gate(mode: &str, root: &Path, fd: i32) {
    if pending_force(mode) {
        // This existing peer offers one input, accepts its stopped close where
        // one is sent, and answers the actual Lower. The controller holds the
        // message.user filing, so the dispatcher must reach no generation.
        return ready_rollback::gate(
            "input-worker-generation-emission-session-forced-save-sqlite-accepted",
            root,
            fd,
        );
    }
    if mode.ends_with("-dispatch-demand-two") {
        wait_file(root, "dispatch-save-complete");
    }
    let mut bytes = vec![0; weaver_types::MAX_ENVELOPE_BYTES];
    for index in 0..2 {
        let input = frame(index, false);
        packet!(fd, &input);
        let n = recv(fd, &mut bytes, MsgFlags::empty()).unwrap();
        assert!(n > 0, "dispatcher ended before the original world close");
        let reply: OrganEnvelope = serde_json::from_slice(&bytes[..n]).unwrap();
        assert_eq!(reply.exchange, input.exchange);
        assert_eq!(reply.position, Position::Close);
        let Payload::Frame(answer) = reply.payload else {
            panic!("the original gate input requires a world close");
        };
        let answer: serde_json::Value = serde_json::from_slice(&answer.octets().unwrap()).unwrap();
        assert_eq!(answer["kind"], "answered");
        assert_eq!(answer["turn"], format!("session-{index}"));
        std::fs::write(
            root.join(format!("session-close-{index}.json")),
            &bytes[..n],
        )
        .unwrap();
    }
    // The existing graceful peer checks the real Quiesce, then Lower and its
    // answer. Only the controller can start that lifecycle after both closes.
    ready_rollback::gate(mode, root, fd);
}

pub(super) fn spu(mode: &str, fd: i32) {
    let root = std::path::PathBuf::from(std::env::var("WEAVER_WORKER_INPUT_ROOT").unwrap());
    let mut bytes = vec![0; weaver_types::MAX_ENVELOPE_BYTES];
    let mut resident = 3;
    for index in if resting(mode) { 2..3 } else { 0..3 } {
        let wind_down = index == 2;
        let text = if wind_down {
            "Read summary."
        } else {
            "Read caf\u{e9}."
        };
        let n = recv(fd, &mut bytes, MsgFlags::empty()).unwrap();
        assert!(n > 0, "dispatcher ended before its measured generation");
        let request: TokenDirective = serde_json::from_slice(&bytes[..n]).unwrap();
        let TokenDirective::MeasureDelta {
            keep,
            delta,
            tool_schemas,
        } = request
        else {
            panic!("measurement must precede each generation");
        };
        assert_eq!(
            keep, resident,
            "the next pass retains the exact journal prefix"
        );
        assert!(tool_schemas.is_empty());
        assert_eq!(
            delta,
            vec![Message {
                role: Role::User,
                content: vec![ContentBlock::Text { text: text.into() }],
            }]
        );
        packet!(
            fd,
            &TokenAnswer::DeltaMeasured {
                resident,
                prefix_tokens: 3,
                keep,
                input_tokens: 3,
                capacity: 16_384,
            }
        );
        let n = recv(fd, &mut bytes, MsgFlags::empty()).unwrap();
        assert!(n > 0, "dispatcher abandoned its measured generation");
        let request: TokenDirective = serde_json::from_slice(&bytes[..n]).unwrap();
        let TokenDirective::AppendAndGenerate {
            turn,
            delta: appended,
            tool_schemas,
        } = request
        else {
            panic!("the measured delta must be consumed by generation");
        };
        assert_eq!(appended, delta);
        assert!(tool_schemas.is_empty());
        assert_eq!(
            turn.0,
            if wind_down {
                "wind-down-0".into()
            } else {
                format!("session-{index}")
            }
        );
        std::fs::write(
            root.join(format!("dispatch-generation-{index}.json")),
            &bytes[..n],
        )
        .unwrap();
        resident += 5;
        let output = if wind_down {
            "Read result."
        } else {
            "Read file."
        };
        packet!(
            fd,
            &TokenAnswer::Generated(Generation {
                emission: output.into(),
                finish: Finish::Completed,
                content: vec![ContentBlock::Text {
                    text: output.into()
                }],
                unrecovered: vec![],
                request: serde_json::value::to_raw_value(&serde_json::json!({
                    "dispatch_generation": index,
                    "wind_down": wind_down,
                }))
                .unwrap(),
                measurement: serde_json::value::RawValue::from_string(
                    "{\"input_tokens\":[17,18,19],\"output_tokens\":[31]}".into(),
                )
                .unwrap(),
                resident,
                capacity: 16_384,
            })
        );
    }
    // Return to residency_composition_child, which retains the actual child
    // and residency socket until the worker closes decode and sends Release.
}
