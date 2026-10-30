//! Spec 07 PLAN-OBSERVATION next_ready through the public worker ports against
//! the actual SQLite member, gate and decoder children: close the first step,
//! consume its receipt, activate the member's successor, place its step
//! loop.instruction, then observe and author next_ready. A Stop before the
//! observation is admitted discards it; a pending admitted ticket resumes.
use super::*;
use crate::flow::context::worker_startup::input::plan::tests as root_plan;
use weaver_types::{Finish, Generation, TokenAnswer, TokenDirective};

use super::activation::{attest, complete, step};

const TURN: &str = "next-ready-turn";

fn packet(fd: i32, value: &TokenAnswer) {
    let bytes = serde_json::to_vec(value).unwrap();
    assert_eq!(
        send(fd, &bytes, MsgFlags::MSG_NOSIGNAL).unwrap(),
        bytes.len()
    );
}

fn generated(finish: Finish) -> TokenAnswer {
    TokenAnswer::Generated(Generation {
        emission: "Read result.".into(),
        finish,
        content: vec![weaver_traits::ContentBlock::Text {
            text: "Read result.".into(),
        }],
        unrecovered: vec![],
        request: serde_json::value::RawValue::from_string(
            "{ \"rendered\": \"fixture\", \"template\": \"fixture\" }".into(),
        )
        .unwrap(),
        measurement: serde_json::value::to_raw_value(&serde_json::json!({
            "model": "fixture",
            "weights_hash": "fixture",
            "input_tokens": [17, 18, 19],
            "output_tokens": [31]
        }))
        .unwrap(),
        resident: 8,
        capacity: 16384,
    })
}

/// The decoder child after Open: measure the step instruction's placement,
/// answer its generation as completed, and answer a Cancel as a stopped
/// generation at rest. It returns when the worker closes the decode socket.
pub(in crate::flow::context::worker_startup) fn spu(mode: &str, fd: i32) {
    let root = std::path::PathBuf::from(std::env::var("WEAVER_WORKER_INPUT_ROOT").unwrap());
    let mut bytes = vec![0; weaver_types::MAX_ENVELOPE_BYTES];
    let end = Instant::now() + Duration::from_secs(20);
    let mut measured = false;
    let mut appended = false;
    while Instant::now() < end {
        match nix::sys::socket::recv(fd, &mut bytes, MsgFlags::MSG_DONTWAIT) {
            Ok(0) => return,
            Ok(n) => {
                if bytes[0] == b'x' {
                    continue;
                }
                let request: TokenDirective = serde_json::from_slice(&bytes[..n]).unwrap();
                match &request {
                    TokenDirective::MeasureDelta { keep, delta, .. } => {
                        assert!(!measured && !appended, "{mode}: one measured placement");
                        std::fs::write(root.join("next-ready-measured.json"), &bytes[..n]).unwrap();
                        assert!(!delta.is_empty());
                        measured = true;
                        packet(
                            fd,
                            &TokenAnswer::DeltaMeasured {
                                resident: 3,
                                prefix_tokens: 3,
                                keep: *keep,
                                input_tokens: 3,
                                capacity: 16384,
                            },
                        );
                    }
                    TokenDirective::AppendAndGenerate { turn, .. } => {
                        assert!(measured && !appended);
                        assert_eq!(turn.0, TURN);
                        appended = true;
                        packet(fd, &generated(Finish::Completed));
                    }
                    TokenDirective::Cancel { turn } => {
                        assert!(appended, "{mode}: cancel follows the held generation");
                        assert_eq!(turn.0, TURN);
                        packet(fd, &generated(Finish::Stopped));
                        packet(fd, &TokenAnswer::AtRest);
                    }
                    _ => panic!("unexpected next_ready decoder request {request:?}"),
                }
            }
            Err(nix::errno::Errno::EAGAIN) => {}
            other => panic!("next_ready decode fixture: {other:?}"),
        }
        std::thread::sleep(Duration::from_millis(1));
    }
    panic!("next_ready decoder child watchdog");
}

fn until(
    input: &mut FoundationInput<'_>,
    member: &mut Member,
    what: &str,
    mut done: impl FnMut(&mut FoundationInput<'_>) -> bool,
) {
    let end = Instant::now() + Duration::from_secs(10);
    while !done(input) {
        step(input, member);
        assert!(Instant::now() < end, "{what} did not settle");
    }
}

fn number(row: &serde_json::Value, field: &str) -> u64 {
    row[field].as_str().unwrap().parse().unwrap()
}

pub(super) fn exercise(
    serving: FoundationServing<'_>,
    member: &mut Member,
    reader: &mut UnixStream,
    socket: &Path,
    mode: &str,
) -> Option<Cleanup> {
    let pending_stop = mode.ends_with("-next-ready-pending-stop");
    let mut guard = root_plan::sqlite_update(serving, member, reader, socket, false);
    std::fs::write(
        socket.parent().unwrap().join("flags-turn-request"),
        b"offer\n",
    )
    .unwrap();
    let input = guard.input.as_mut().unwrap();
    until(input, member, "gate input", |input| {
        input.progress() == FoundationInputProgress::Accepted
    });
    let wire = input.received.as_ref().unwrap().raw_bytes().to_vec();
    let turn = TurnKey(TURN.into());
    input.start_task(turn.clone()).unwrap();
    until(input, member, "task", |input| {
        input.progress() == FoundationInputProgress::Task
    });
    let plan = input
        .serving
        .owner
        .context
        .committed_plan_summary()
        .unwrap()
        .unwrap()
        .plan
        .clone();
    // Activate and close the first step against the actual member.
    input
        .step_successor(plan.clone(), None, Some(turn.clone()))
        .unwrap();
    let checked = complete(input, member).into_successor().unwrap();
    let first = checked.reference().step_id.clone();
    input
        .activate(plan.clone(), first.clone(), checked, turn.clone())
        .unwrap();
    let activated = complete(input, member);
    let FoundationPlanOutcome::Activated(active) = activated.outcome() else {
        panic!("the first activation commits")
    };
    let span = active.span.clone().unwrap();
    let evidence = attest(input, member, &plan, &first, span, &turn);
    input
        .close(plan.clone(), first.clone(), evidence, turn.clone())
        .unwrap();
    let receipt = complete(input, member)
        .into_closure_receipt()
        .unwrap_or_else(|_| panic!("a committed close grants its receipt"));
    let closed = receipt.closed().clone();
    let start = input.begin_next_ready(receipt).unwrap();
    // The member's successor after the closed step, and its activation.
    input
        .step_successor(plan.clone(), Some(first.clone()), Some(turn.clone()))
        .unwrap();
    let checked = complete(input, member).into_successor().unwrap();
    let second = checked.reference().step_id.clone();
    assert_ne!(second, first);
    input
        .activate(plan.clone(), second.clone(), checked, turn.clone())
        .unwrap();
    let activation = complete(input, member)
        .into_activation_receipt()
        .unwrap_or_else(|_| panic!("a committed activation grants its receipt"));
    assert_eq!(activation.step().step_id, second);
    let step_span = activation.step().span.clone().unwrap();
    let instruction = activation.step().instruction.clone();
    // Place its step loop.instruction through the engine's admission.
    assert_eq!(input.progress(), FoundationInputProgress::Task);
    input
        .place_step_instruction(
            vec![],
            plan.clone(),
            second.clone(),
            turn.clone(),
            "next ready".into(),
            s::CutMethod::Flush,
        )
        .unwrap();
    let mut placed = None;
    until(input, member, "step placement", |input| {
        assert!(
            input.take_step_refusal().is_none(),
            "the step placement refused"
        );
        placed = input.take_step_placement();
        placed.is_some()
    });
    let placed = placed.unwrap();
    assert_eq!(input.progress(), FoundationInputProgress::Placed);
    assert_eq!(placed.step_id(), &second);
    assert_eq!(placed.instruction(), &instruction);
    assert_eq!(placed.span(), &step_span);
    assert!(crate::flow::material::is_instruction_unit(
        &placed.unit,
        placed.source(),
        input.serving.owner.context,
    ));
    let source = placed.source().clone();
    let placement = placed.placement();
    let ended_ns = placed.ended_ns;
    input.observe_next_ready(start, activation, placed).unwrap();
    assert!(input.next_ready_pending());
    // The outstanding placement grant holds external authorship: the
    // observation yields to it rather than taking the slot.
    for _ in 0..8 {
        step(input, member);
    }
    assert!(input.next_ready_pending() && !input.next_ready_started());
    let mut recorded = None;
    let mut drained = vec![];
    if !pending_stop {
        input.generate().unwrap();
        // The observation yields until the generation releases external
        // authorship. Then, under a full recorder, its admitted ticket stays
        // pending across interrupted waits and every retry resumes that same
        // ticket, never a rebuilt event, until the queue drains.
        until(input, member, "generation", |input| {
            !input.serving.owner.context.place_blocks_external()
        });
        assert!(!input.next_ready_started());
        pressure::fill(input.serving.owner.context);
        let mut ticket = None;
        for _ in 0..2 {
            let context = &mut input.serving.owner.context;
            let interrupt = AbsoluteBound::after(MonotonicIo.now().unwrap(), 100_000_000).unwrap();
            context
                .arm(PumpDeadline::Label, interrupt, &mut MonotonicIo)
                .unwrap();
            assert!(matches!(
                input.advance_next_ready(None, &mut MonotonicIo),
                Err(RunFailure::Wait { key: PumpDeadline::Label, bound }) if bound == interrupt
            ));
            let context = &mut input.serving.owner.context;
            context.complete_wait(PumpDeadline::Label).unwrap();
            assert!(input.next_ready_started() && input.next_ready_pending());
            let held = input.serving.owner.context.pending_author().unwrap().ticket;
            assert_eq!(
                *ticket.get_or_insert(held),
                held,
                "the retry resumes its ticket"
            );
        }
        drained = pressure::drain(input.serving.owner.context, reader);
        until(input, member, "next_ready record", |input| {
            recorded = input.take_next_ready();
            recorded.is_some()
        });
    }
    // Stop the turn.
    let peer = dial(socket);
    let envelope = OrganEnvelope {
        exchange: ExchangeId {
            opener: Opener::Admin,
            ordinal: 417,
        },
        position: Position::Open,
        payload: Payload::Directive(LifecycleDirective::Stop {
            cause: Cause { uid: 0 },
        }),
    };
    let bytes = serde_json::to_vec(&envelope).unwrap();
    assert_eq!(
        send(peer.as_raw_fd(), &bytes, MsgFlags::MSG_NOSIGNAL).unwrap(),
        bytes.len()
    );
    let end = Instant::now() + Duration::from_secs(10);
    loop {
        let input = guard.input.as_mut().unwrap();
        let progress = input.advance_with_io(&mut NonblockingIo { waits: 0 });
        assert_ne!(
            progress,
            FoundationInputProgress::Stopped,
            "{:?}",
            input.detail()
        );
        if progress == FoundationInputProgress::ManagementPending {
            break;
        }
        member.step_live("ready");
        assert!(Instant::now() < end, "Stop was not admitted");
    }
    guard.management = Some(guard.input.take().unwrap().management().ok().unwrap());
    loop {
        let owner = guard.management.as_mut().unwrap();
        let progress = owner.advance_with_io(&mut NonblockingIo { waits: 0 });
        assert_ne!(
            progress,
            FoundationManagementProgress::Stopped,
            "{:?}",
            owner.detail()
        );
        if progress == FoundationManagementProgress::Answered {
            break;
        }
        member.step_live("ready");
        assert!(Instant::now() < end, "Stop did not settle");
        std::thread::sleep(Duration::from_millis(1));
    }
    let answer: OrganEnvelope = serde_json::from_slice(&read_packet(&peer).unwrap()).unwrap();
    assert_eq!(answer.exchange, envelope.exchange);
    assert_eq!(
        answer.payload,
        Payload::Answer(LifecycleAnswer::TurnAborted { turn: turn.clone() })
    );
    guard.input = Some(guard.management.take().unwrap().finish().ok().unwrap());
    let input = guard.input.as_mut().unwrap();
    assert!(!input.next_ready_pending());
    assert!(input.take_next_ready().is_none());
    let through = input
        .serving
        .owner
        .context
        .barrier
        .latest_recorded()
        .unwrap();
    let rows = trace_through(
        reader,
        drained,
        through,
        Instant::now() + Duration::from_secs(5),
    )
    .unwrap();
    // The filed step loop.instruction names the activated successor.
    let instructions: Vec<_> = rows
        .iter()
        .filter(|row| row["kind"] == "loop.instruction")
        .collect();
    assert_eq!(instructions.len(), 1, "{rows:#?}");
    assert_eq!(instructions[0]["turn"], TURN);
    assert_eq!(number(instructions[0], "sequence"), source.sequence.0);
    assert_eq!(
        instructions[0]["payload"]["template_id"],
        "step.instruction"
    );
    assert_eq!(
        instructions[0]["payload"]["instruction"],
        serde_json::json!({"step": {
            "plan": serde_json::to_value(&plan).unwrap(),
            "step_id": serde_json::to_value(&second).unwrap(),
            "instruction": serde_json::to_value(&instruction).unwrap(),
            "span": serde_json::to_value(&step_span).unwrap(),
        }})
    );
    // Its Recorded placement.
    let placements: Vec<_> = rows
        .iter()
        .filter(|row| row["kind"] == "state.placement" && number(row, "sequence") == placement.0)
        .collect();
    assert_eq!(placements.len(), 1, "{rows:#?}");
    let measurements: Vec<_> = rows
        .iter()
        .filter(|row| row["kind"] == "state.plan.measurement")
        .filter(|row| !row["payload"]["event"]["next_ready"].is_null())
        .collect();
    if pending_stop {
        // Discarded unadmitted at Stop: nothing authored.
        assert!(measurements.is_empty(), "{measurements:#?}");
        assert!(recorded.is_none());
    } else {
        assert_eq!(measurements.len(), 1, "one admitted, resumed ticket");
        let row = measurements[0];
        assert_eq!(Some(number(row, "sequence")), recorded.map(|s| s.0));
        assert_eq!(row["turn"], TURN);
        assert_eq!(number(row, "causal_parent"), placement.0);
        let event = &row["payload"]["event"]["next_ready"];
        assert_eq!(event["closed"], serde_json::to_value(&closed).unwrap());
        assert_eq!(
            event["previous_step"],
            serde_json::to_value(&first).unwrap()
        );
        assert_eq!(event["step_id"], serde_json::to_value(&second).unwrap());
        assert_eq!(event["ended_ns"], ended_ns.to_string());
        let began_ns: u64 = event["began_ns"].as_str().unwrap().parse().unwrap();
        assert!(began_ns > 0 && began_ns < ended_ns);
        assert_eq!(
            event["closure_to_next_ns"],
            (ended_ns - began_ns).to_string()
        );
    }
    let closes: Vec<_> = rows
        .iter()
        .filter(|row| row["kind"] == "turn.closed")
        .collect();
    assert_eq!(closes.len(), 1);
    assert_eq!(closes[0]["turn"], TURN);
    let close_path = socket.parent().unwrap().join("flags-turn-close.json");
    while !close_path.exists() {
        assert!(
            Instant::now() < end,
            "actual gate did not receive the stopped world reply"
        );
        std::thread::sleep(Duration::from_millis(1));
    }
    let close: OrganEnvelope = serde_json::from_slice(&std::fs::read(close_path).unwrap()).unwrap();
    let original: OrganEnvelope = serde_json::from_slice(&wire).unwrap();
    assert_eq!(close.exchange, original.exchange);
    assert_eq!(close.position, Position::Close);
    None
}
