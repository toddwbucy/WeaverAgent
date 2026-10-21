//! Private owner tests. Checks and receipts use the earlier real ledger fixtures.
//! Every gate packet is received through a real Unix socket, never constructed
//! as a ReceivedOrgan or treated as a filed result.

use super::super::repair::RepairLedger;
use super::tests::{
    accepted, binding, call, committed, envelope, event_id, open, original, source,
    statement_receipt, text,
};
use super::*;
use crate::flow::pump_transport::OrganPort;
use nix::sys::socket::{AddressFamily, MsgFlags, SockFlag, SockType, send, socketpair};
use std::os::fd::AsRawFd;
use weaver_trace::Sequence;
use weaver_trace::serving::{AdmittedCopy, PairCopy, SelectedBlock, SourceBlock};

fn make_effective(
    content: Vec<ContentBlock>,
    results: Vec<Ste100Result>,
) -> (SchemeDeclaration, EffectiveEmission) {
    let (scheme, original) = original(content, results);
    let mut repair = RepairLedger::begin(original, 2).unwrap();
    (scheme, repair.take_effective().unwrap().unwrap())
}

fn one() -> (PairLedger, CallId, SchemeDeclaration) {
    let (scheme, effective) = make_effective(
        vec![text("Read files."), call("ls")],
        vec![accepted("Read files.")],
    );
    let call = effective.requests()[0].call_id.clone();
    let mut ledger = PairLedger::new("session".into(), "run".into(), scheme.clone()).unwrap();
    ledger.accept_effective(effective).unwrap();
    (ledger, call, scheme)
}

fn statement(ledger: &mut PairLedger, scheme: &SchemeDeclaration) {
    ledger
        .install_statement(statement_receipt(
            scheme,
            source(0),
            "Read files.",
            vec![binding("Read files.")],
        ))
        .unwrap();
}

fn ready() -> (PairLedger, CallId, SchemeDeclaration) {
    let (mut ledger, call, scheme) = one();
    statement(&mut ledger, &scheme);
    open(&mut ledger, &call, &scheme, 30);
    (ledger, call, scheme)
}

fn exchange(ordinal: u64) -> ExchangeId {
    ExchangeId {
        opener: Opener::Harness,
        ordinal,
    }
}

fn received_raw(raw: &[u8]) -> ReceivedOrgan {
    let (end, peer) = socketpair(
        AddressFamily::Unix,
        SockType::SeqPacket,
        None,
        SockFlag::SOCK_CLOEXEC,
    )
    .unwrap();
    let mut port = OrganPort::from_owned(end).unwrap();
    assert_eq!(
        send(peer.as_raw_fd(), raw, MsgFlags::MSG_NOSIGNAL).unwrap(),
        raw.len()
    );
    port.try_receive().unwrap();
    port.take_received().unwrap().unwrap()
}

fn received(ordinal: u64, outcome: ToolOutcome) -> ReceivedOrgan {
    received_raw(
        &serde_json::to_vec(&OrganEnvelope {
            exchange: exchange(ordinal),
            position: Position::Close,
            payload: weaver_types::Payload::ToolAnswer(outcome),
        })
        .unwrap(),
    )
}

fn result() -> ToolOutcome {
    ToolOutcome::Result {
        content: "exact complete result".into(),
    }
}

fn dispatch(ledger: &mut PairLedger, call: &CallId, ordinal: u64) {
    let frame = ledger
        .bind_execution(call, exchange(ordinal), 1000)
        .unwrap();
    assert_eq!(frame.exchange, exchange(ordinal));
    assert!(matches!(
        frame.payload,
        weaver_types::Payload::Tool(ToolExecution { clock_ms: 1000, .. })
    ));
    ledger.mark_sent(call, &exchange(ordinal)).unwrap();
}

fn terminal_receipt(
    scheme: &SchemeDeclaration,
    sequence: u64,
    ended: &PairEnded,
    gate: bool,
) -> CommittedSource {
    let raw_envelope = envelope(
        sequence,
        "tool.call.completed",
        "tool",
        if gate { "tool" } else { "loop" },
    );
    let raw_envelope: Arc<str> = if gate {
        raw_envelope
            .replace("\"subsystem\":\"harness\"", "\"subsystem\":\"gate\"")
            .into()
    } else {
        raw_envelope
    };
    let copy = AdmittedCopy::Pair(PairCopy {
        source_envelope: raw_envelope,
        partition_rule_id: "pairs".into(),
        block: SelectedBlock {
            source: SourceBlock {
                run: "run".into(),
                sequence: Sequence(sequence),
                block: 0,
            },
            source_path: vec![],
            raw: serde_json::to_string(ended).unwrap().into(),
        },
    });
    committed(scheme, event_id(sequence), copy, None)
}

fn close(ledger: &mut PairLedger, call: &CallId, scheme: &SchemeDeclaration, sequence: u64) {
    let ending = ledger.next_terminal().unwrap().1.clone();
    let gate = ledger.terminal_is_gate(call).unwrap();
    ledger
        .mark_authoring(call, PairRecordPhase::Terminal)
        .unwrap();
    ledger
        .mark_recorded(call, PairRecordPhase::Terminal, event_id(sequence))
        .unwrap();
    ledger
        .install_terminal(call, terminal_receipt(scheme, sequence, &ending, gate))
        .unwrap();
}

#[test]
fn execution_requires_effective_owner_and_each_of_the_two_commits() {
    let (scheme, original) = original(
        vec![text("Read files."), call("ls")],
        vec![accepted("Read files.")],
    );
    let call = original.requests()[0].opening().call_id.clone();
    let mut ordinary = PairLedger::new("session".into(), "run".into(), scheme.clone()).unwrap();
    ordinary.accept(original).unwrap();
    statement(&mut ordinary, &scheme);
    open(&mut ordinary, &call, &scheme, 30);
    assert!(ordinary.next_ready().is_none());
    assert_eq!(
        ordinary
            .bind_execution(&call, exchange(1), 1000)
            .unwrap_err(),
        PairError::WrongPhase
    );

    let (mut ledger, call, scheme) = one();
    assert_eq!(
        ledger.bind_execution(&call, exchange(1), 1000).unwrap_err(),
        PairError::WrongPhase
    );
    open(&mut ledger, &call, &scheme, 30);
    assert_eq!(
        ledger.bind_execution(&call, exchange(1), 1000).unwrap_err(),
        PairError::WrongPhase
    );
    statement(&mut ledger, &scheme);
    dispatch(&mut ledger, &call, 1);

    let (mut ledger, call, scheme) = one();
    statement(&mut ledger, &scheme);
    assert_eq!(
        ledger.bind_execution(&call, exchange(1), 1000).unwrap_err(),
        PairError::WrongPhase
    );
    open(&mut ledger, &call, &scheme, 30);
    dispatch(&mut ledger, &call, 1);
}

#[test]
fn matched_outcome_is_not_a_completed_pair_before_the_exact_terminal_receipt() {
    let (mut ledger, call, scheme) = ready();
    dispatch(&mut ledger, &call, 41);
    let packet = received(41, result());
    let raw = packet.raw_bytes().to_vec();
    assert_eq!(
        ledger.observe_gate(packet).unwrap(),
        GateObservation::Matched(call.clone())
    );
    assert!(ledger.completed(&call).unwrap().is_none());
    assert_eq!(
        ledger.finish_effective().unwrap_err(),
        PairError::WrongPhase
    );
    let ended = ledger.next_terminal().unwrap().1.clone();
    ledger
        .mark_authoring(&call, PairRecordPhase::Terminal)
        .unwrap();
    ledger
        .mark_recorded(&call, PairRecordPhase::Terminal, event_id(40))
        .unwrap();
    let (_, error) = ledger
        .install_terminal(&call, terminal_receipt(&scheme, 40, &ended, false))
        .unwrap_err();
    assert_eq!(error, PairError::WrongReceipt);
    assert!(ledger.completed(&call).unwrap().is_none());
    ledger
        .install_terminal(&call, terminal_receipt(&scheme, 40, &ended, true))
        .unwrap();
    assert_eq!(
        ledger.completed(&call).unwrap().unwrap().raw_answer(),
        Some(raw.as_slice())
    );
    assert_eq!(ledger.finish_batch().unwrap_err(), PairError::WrongPhase);
    ledger.finish_effective().unwrap();
    assert_eq!(
        ledger.completed(&call).unwrap().unwrap().ended().fate,
        TerminalPairFate::Completed
    );
}

#[test]
fn every_gate_terminal_mapping_keeps_the_whole_outcome() {
    for (outcome, reason) in [
        (result(), None),
        (
            ToolOutcome::Refused {
                reason: "exact refusal".into(),
            },
            None,
        ),
        (
            ToolOutcome::Errored {
                detail: "exact error".into(),
            },
            None,
        ),
        (
            ToolOutcome::Killed {
                by: KillCause::Clock,
                partial: Some("partial".into()),
            },
            Some(PairFailure::Timeout {}),
        ),
        (
            ToolOutcome::Killed {
                by: KillCause::Cancel,
                partial: None,
            },
            Some(PairFailure::Cancelled {}),
        ),
        (
            ToolOutcome::Killed {
                by: KillCause::Unload,
                partial: None,
            },
            Some(PairFailure::Unload {}),
        ),
    ] {
        let (mut ledger, call, scheme) = ready();
        dispatch(&mut ledger, &call, 1);
        ledger.observe_gate(received(1, outcome.clone())).unwrap();
        close(&mut ledger, &call, &scheme, 40);
        let pair = ledger.completed(&call).unwrap().unwrap();
        assert_eq!(pair.ended().outcome.as_ref(), Some(&outcome));
        assert_eq!(pair.ended().reason, reason);
        assert_eq!(
            pair.ended().fate,
            if reason.is_some() {
                TerminalPairFate::Failed
            } else {
                TerminalPairFate::Completed
            }
        );
    }
}

#[test]
fn rejection_keeps_exact_wire_until_actual_recorded_disposition() {
    let (mut ledger, call, _) = ready();
    dispatch(&mut ledger, &call, 1);
    // Encode the real wire shape, then add insignificant internal whitespace.
    let encoded = serde_json::to_string(&OrganEnvelope {
        exchange: exchange(97),
        position: Position::Close,
        payload: weaver_types::Payload::ToolAnswer(result()),
    })
    .unwrap();
    let encoded = format!(" \n {} \t", encoded.replacen('{', "{  ", 1));
    ledger
        .observe_gate(received_raw(encoded.as_bytes()))
        .unwrap();
    let pending = ledger.pending_rejection().unwrap();
    assert_eq!(pending.raw_bytes(), encoded.as_bytes());
    assert!(pending.record().raw_envelope.get().contains("{  "));
    assert_eq!(pending.record().reason, "unexpected_exchange");
    assert!(pending.record().call_id.is_none());
    let (held, error) = ledger.observe_gate(received(1, result())).unwrap_err();
    assert_eq!(error, PairError::Busy);
    assert_eq!(
        ledger
            .ack_rejection_recorded(SourcePosition {
                block_id: SourceBlockId {
                    run: "wrong".into(),
                    ..event_id(50)
                },
                turn: Some("turn".into())
            })
            .unwrap_err(),
        PairError::InvalidIdentity
    );
    assert!(ledger.pending_rejection().is_some());
    let raw_owner = ledger
        .ack_rejection_recorded(SourcePosition {
            block_id: event_id(50),
            turn: Some("turn".into()),
        })
        .unwrap();
    assert_eq!(raw_owner.raw_bytes(), encoded.as_bytes());
    assert_eq!(
        ledger.observe_gate(held).unwrap(),
        GateObservation::Matched(call)
    );
}

#[test]
fn exact_exchange_close_and_sent_state_are_required() {
    let (mut ledger, call, _) = ready();
    ledger.bind_execution(&call, exchange(1), 1000).unwrap();
    assert_eq!(
        ledger.close_gate_fault(&call).unwrap_err(),
        PairError::WrongPhase
    );
    assert!(ledger.next_terminal().is_none());
    ledger.observe_gate(received(1, result())).unwrap();
    assert_eq!(
        ledger.pending_rejection().unwrap().record().reason,
        "unexpected_exchange"
    );
    assert!(ledger.next_terminal().is_none());
    ledger
        .ack_rejection_recorded(SourcePosition {
            block_id: event_id(40),
            turn: Some("turn".into()),
        })
        .unwrap();
    ledger.mark_sent(&call, &exchange(1)).unwrap();
    assert_eq!(
        ledger.mark_sent(&call, &exchange(1)).unwrap_err(),
        PairError::WrongPhase
    );
    for position in [Position::Open, Position::Continue] {
        let packet = received_raw(
            &serde_json::to_vec(&OrganEnvelope {
                exchange: exchange(1),
                position,
                payload: weaver_types::Payload::ToolAnswer(result()),
            })
            .unwrap(),
        );
        assert_eq!(
            ledger.observe_gate(packet).unwrap_err().1,
            PairError::Protocol
        );
    }
    let packet = received_raw(
        &serde_json::to_vec(&OrganEnvelope {
            exchange: ExchangeId {
                opener: Opener::Gate,
                ordinal: 1,
            },
            position: Position::Close,
            payload: weaver_types::Payload::ToolAnswer(result()),
        })
        .unwrap(),
    );
    ledger.observe_gate(packet).unwrap();
    assert!(
        ledger
            .pending_rejection()
            .unwrap()
            .record()
            .call_id
            .is_none()
    );
    assert!(ledger.next_terminal().is_none());
}

#[test]
fn gate_cannot_mint_fault_or_flow_failure_on_healthy_flow() {
    let (mut ledger, call, _) = ready();
    dispatch(&mut ledger, &call, 1);
    for by in [KillCause::Fault, KillCause::FlowFailure] {
        assert_eq!(
            ledger
                .observe_gate(received(1, ToolOutcome::Killed { by, partial: None }))
                .unwrap_err()
                .1,
            PairError::Protocol
        );
        assert!(ledger.next_terminal().is_none());
    }
}

#[test]
fn stop_and_leave_do_not_rewrite_an_execution_or_its_first_result() {
    let (mut ledger, call, scheme) = ready();
    dispatch(&mut ledger, &call, 1);
    ledger.set_not_dispatched(NonDispatchCause::Stop).unwrap();
    assert!(ledger.next_terminal().is_none());
    ledger.observe_gate(received(1, result())).unwrap();
    ledger.set_not_dispatched(NonDispatchCause::Unload).unwrap();
    assert_eq!(
        ledger.next_terminal().unwrap().1.fate,
        TerminalPairFate::Completed
    );
    assert_eq!(
        ledger.close_gate_fault(&call).unwrap_err(),
        PairError::WrongPhase
    );
    close(&mut ledger, &call, &scheme, 40);
}

#[test]
fn duplicate_and_late_closes_are_rejected_after_terminal_commit_and_batch_release() {
    for fault in [false, true] {
        let (mut ledger, call, scheme) = ready();
        dispatch(&mut ledger, &call, 1);
        if fault {
            ledger.close_gate_fault(&call).unwrap();
        } else {
            ledger.observe_gate(received(1, result())).unwrap();
        }
        close(&mut ledger, &call, &scheme, 40);
        ledger.finish_effective().unwrap();
        ledger.observe_gate(received(1, result())).unwrap();
        let rejection = ledger.pending_rejection().unwrap().record();
        assert_eq!(
            rejection.reason,
            if fault {
                "late_return"
            } else {
                "duplicate_return"
            }
        );
        assert_eq!(
            rejection.call_id.as_ref().unwrap().sequence.0,
            call.sequence.0
        );
        assert_eq!(
            ledger.completed(&call).unwrap().unwrap().ended().reason,
            if fault {
                Some(PairFailure::GateFault {})
            } else {
                None
            }
        );
    }
}

#[test]
fn freeze_retains_matching_outcome_for_trace_without_filing_or_grant() {
    let (mut ledger, call, _) = ready();
    dispatch(&mut ledger, &call, 1);
    assert_eq!(ledger.outstanding()[0].state, PairState::Executing);
    assert_eq!(ledger.outstanding()[0].execution, Some(exchange(1)));
    ledger.freeze();
    assert_eq!(
        ledger
            .observe_gate(received(
                1,
                ToolOutcome::Killed {
                    by: KillCause::FlowFailure,
                    partial: Some("last bytes".into())
                }
            ))
            .unwrap(),
        GateObservation::Halted(call.clone())
    );
    assert_eq!(
        ledger.halted_terminal().unwrap().1.reason,
        Some(PairFailure::FlowFailure {})
    );
    assert!(ledger.next_terminal().is_none());
    assert_eq!(ledger.completed(&call).unwrap_err(), PairError::Frozen);
    assert_eq!(ledger.finish_effective().unwrap_err(), PairError::Frozen);
}

#[test]
fn ungranted_closure_has_no_exchange_and_requires_terminal_receipt() {
    let (mut ledger, call, scheme) = ready();
    ledger
        .refuse_ungranted(&call, "tool not elected".into())
        .unwrap();
    assert!(ledger.executing().is_none());
    assert!(!ledger.terminal_is_gate(&call).unwrap());
    assert!(ledger.completed(&call).unwrap().is_none());
    close(&mut ledger, &call, &scheme, 40);
    assert_eq!(
        ledger.completed(&call).unwrap().unwrap().ended().reason,
        Some(PairFailure::NotGranted {})
    );
}

#[test]
fn ready_order_waits_for_prior_terminal_and_never_skips_inward_lane() {
    let (scheme, effective) = make_effective(
        vec![
            text("Read files."),
            call("ls"),
            text("Read names."),
            call("pwd"),
        ],
        vec![accepted("Read files."), accepted("Read names.")],
    );
    let calls: Vec<_> = effective
        .requests()
        .iter()
        .map(|request| request.call_id.clone())
        .collect();
    let mut ledger = PairLedger::new("session".into(), "run".into(), scheme.clone()).unwrap();
    ledger.accept_effective(effective).unwrap();
    statement(&mut ledger, &scheme);
    ledger
        .install_statement(statement_receipt(
            &scheme,
            source(2),
            "Read names.",
            vec![binding("Read names.")],
        ))
        .unwrap();
    open(&mut ledger, &calls[0], &scheme, 30);
    open(&mut ledger, &calls[1], &scheme, 31);
    assert_eq!(ledger.next_ready().unwrap().call_id, calls[0]);
    assert_eq!(
        ledger
            .bind_execution(&calls[1], exchange(2), 1000)
            .unwrap_err(),
        PairError::WrongPhase
    );
    dispatch(&mut ledger, &calls[0], 1);
    ledger.observe_gate(received(1, result())).unwrap();
    assert!(ledger.next_ready().is_none());
    close(&mut ledger, &calls[0], &scheme, 40);
    assert_eq!(ledger.next_ready().unwrap().call_id, calls[1]);
    assert_eq!(
        ledger
            .bind_execution(&calls[1], exchange(1), 1000)
            .unwrap_err(),
        PairError::WrongExchange
    );

    let (scheme, effective) = make_effective(
        vec![text("Recall files."), text("Read files."), call("ls")],
        vec![accepted("Recall files."), accepted("Read files.")],
    );
    let calls: Vec<_> = effective
        .requests()
        .iter()
        .map(|request| request.call_id.clone())
        .collect();
    let mut ledger = PairLedger::new("session".into(), "run".into(), scheme.clone()).unwrap();
    ledger.accept_effective(effective).unwrap();
    ledger
        .install_statement(statement_receipt(
            &scheme,
            source(0),
            "Recall files.",
            vec![binding("Recall files.")],
        ))
        .unwrap();
    open(&mut ledger, &calls[0], &scheme, 30);
    assert_eq!(ledger.next_ready().unwrap().lane, PairLane::Recall);
    assert_eq!(
        ledger
            .bind_execution(&calls[0], exchange(1), 1000)
            .unwrap_err(),
        PairError::UnsupportedLane
    );
}

#[test]
fn invalid_clock_and_opener_never_bind_execution() {
    let (mut ledger, call, _) = ready();
    for clock in [0, SHELL_MAX_CLOCK_MS + 1] {
        assert_eq!(
            ledger
                .bind_execution(&call, exchange(1), clock)
                .unwrap_err(),
            PairError::InvalidClock
        );
        assert!(ledger.executing().is_none());
    }
    for bad in [
        exchange(0),
        ExchangeId {
            opener: Opener::Gate,
            ordinal: 1,
        },
    ] {
        assert_eq!(
            ledger.bind_execution(&call, bad, 1000).unwrap_err(),
            PairError::WrongExchange
        );
        assert!(ledger.executing().is_none());
    }
}

fn later_effective(sequence: u64, arguments: &str) -> EffectiveEmission {
    let (_, declaration, scheme) = super::tests::fixture_scheme();
    let original = OriginalRequests::from_checked(
        crate::flow::coordinator::checked_request_fixture_at(
            &declaration,
            vec![text("Read files."), call(arguments)],
            vec![accepted("Read files.")],
            "run",
            "turn",
            sequence,
        ),
        &scheme,
    )
    .unwrap();
    RepairLedger::begin(original, 2)
        .unwrap()
        .take_effective()
        .unwrap()
        .unwrap()
}

#[test]
fn retry_requires_a_committed_same_run_external_predecessor_with_exact_request() {
    let (mut ledger, first, scheme) = ready();
    dispatch(&mut ledger, &first, 1);
    ledger.observe_gate(received(1, result())).unwrap();
    close(&mut ledger, &first, &scheme, 40);
    ledger.finish_effective().unwrap();

    let later = later_effective(100, "ls");
    let call = later.requests()[0].call_id.clone();
    ledger.accept_effective(later).unwrap();
    assert_eq!(
        ledger.set_retry_of(&call, &call).unwrap_err(),
        PairError::InvalidLineage
    );
    assert_eq!(
        ledger
            .set_retry_of(
                &call,
                &CallId {
                    run: "other".into(),
                    ..first.clone()
                }
            )
            .unwrap_err(),
        PairError::InvalidLineage
    );
    ledger.set_retry_of(&call, &first).unwrap();
    assert_eq!(
        ledger.next_opening().unwrap().1.retry_of.as_ref(),
        Some(&first)
    );
    assert_eq!(
        ledger.set_retry_of(&call, &first).unwrap_err(),
        PairError::InvalidLineage
    );
    ledger.set_not_dispatched(NonDispatchCause::Stop).unwrap();
    ledger
        .install_statement(statement_receipt(
            &scheme,
            event_id(100),
            "Read files.",
            vec![binding("Read files.")],
        ))
        .unwrap();
    open(&mut ledger, &call, &scheme, 120);
    close(&mut ledger, &call, &scheme, 130);
    ledger.finish_effective().unwrap();

    let changed = later_effective(200, "different bytes");
    let changed_call = changed.requests()[0].call_id.clone();
    ledger.accept_effective(changed).unwrap();
    assert_eq!(
        ledger.set_retry_of(&changed_call, &first).unwrap_err(),
        PairError::InvalidLineage
    );
    assert!(ledger.next_opening().unwrap().1.retry_of.is_none());
}

#[test]
fn an_account_from_successful_repair_still_is_not_dispatch_authority() {
    let (scheme, effective) = make_effective(
        vec![text("Read files."), call("ls")],
        vec![accepted("Read files.")],
    );
    let call = effective.requests()[0].call_id.clone();
    let mut ledger = PairLedger::new("session".into(), "run".into(), scheme.clone()).unwrap();
    ledger.accept_repair(effective.into_account()).unwrap();
    statement(&mut ledger, &scheme);
    open(&mut ledger, &call, &scheme, 30);
    assert!(ledger.next_ready().is_none());
    assert_eq!(
        ledger.bind_execution(&call, exchange(1), 1000).unwrap_err(),
        PairError::WrongPhase
    );
}

#[test]
fn gate_failure_bars_later_dispatch_even_after_the_failed_batch_is_released() {
    let (mut ledger, call, scheme) = ready();
    dispatch(&mut ledger, &call, 1);
    ledger.close_gate_fault(&call).unwrap();
    close(&mut ledger, &call, &scheme, 40);
    ledger.finish_effective().unwrap();
    let next = later_effective(100, "ls");
    let next_call = next.requests()[0].call_id.clone();
    ledger.accept_effective(next).unwrap();
    ledger
        .install_statement(statement_receipt(
            &scheme,
            event_id(100),
            "Read files.",
            vec![binding("Read files.")],
        ))
        .unwrap();
    open(&mut ledger, &next_call, &scheme, 120);
    assert!(ledger.next_ready().is_none());
    assert_eq!(
        ledger
            .bind_execution(&next_call, exchange(2), 1000)
            .unwrap_err(),
        PairError::WrongPhase
    );
    ledger.set_not_dispatched(NonDispatchCause::NoGate).unwrap();
    close(&mut ledger, &next_call, &scheme, 130);
    assert_eq!(
        ledger
            .completed(&next_call)
            .unwrap()
            .unwrap()
            .ended()
            .reason,
        Some(PairFailure::NotDispatched {
            cause: NonDispatchCause::NoGate
        })
    );
}

#[test]
fn stop_after_binding_requires_exact_unsent_withdrawal_and_retires_the_exchange() {
    let (mut ledger, call, scheme) = ready();
    ledger.bind_execution(&call, exchange(1), 1000).unwrap();
    assert_eq!(
        ledger.cancel_unsent(&call, &exchange(1)).unwrap_err(),
        PairError::WrongPhase
    );
    ledger.set_not_dispatched(NonDispatchCause::Stop).unwrap();
    assert_eq!(
        ledger.mark_sent(&call, &exchange(1)).unwrap_err(),
        PairError::WrongPhase
    );
    assert_eq!(
        ledger.cancel_unsent(&call, &exchange(2)).unwrap_err(),
        PairError::WrongPhase
    );
    assert_eq!(ledger.executing().unwrap().1, &exchange(1));
    assert!(ledger.next_terminal().is_none());
    ledger.set_not_dispatched(NonDispatchCause::Unload).unwrap();
    ledger.cancel_unsent(&call, &exchange(1)).unwrap();
    assert!(ledger.executing().is_none());
    assert_eq!(
        ledger.cancel_unsent(&call, &exchange(1)).unwrap_err(),
        PairError::WrongPhase
    );
    let terminal = ledger.next_terminal().unwrap().1;
    assert!(terminal.execution.is_none());
    assert!(terminal.outcome.is_none());
    assert_eq!(
        terminal.reason,
        Some(PairFailure::NotDispatched {
            cause: NonDispatchCause::Unload
        })
    );
    close(&mut ledger, &call, &scheme, 40);
    ledger.finish_effective().unwrap();

    ledger.observe_gate(received(1, result())).unwrap();
    let rejection = ledger.pending_rejection().unwrap().record();
    assert_eq!(rejection.reason, "unexpected_exchange");
    assert_eq!(
        rejection.call_id.as_ref().unwrap().sequence.0,
        call.sequence.0
    );
    ledger
        .ack_rejection_recorded(SourcePosition {
            block_id: event_id(50),
            turn: Some("turn".into()),
        })
        .unwrap();
    let next = later_effective(100, "ls");
    let next_call = next.requests()[0].call_id.clone();
    ledger.accept_effective(next).unwrap();
    ledger
        .install_statement(statement_receipt(
            &scheme,
            event_id(100),
            "Read files.",
            vec![binding("Read files.")],
        ))
        .unwrap();
    open(&mut ledger, &next_call, &scheme, 120);
    assert_eq!(
        ledger
            .bind_execution(&next_call, exchange(1), 1000)
            .unwrap_err(),
        PairError::WrongExchange
    );
    dispatch(&mut ledger, &next_call, 2);
}

#[test]
fn a_sent_execution_cannot_be_withdrawn_or_replaced_by_a_stop_terminal() {
    let (mut ledger, call, scheme) = ready();
    dispatch(&mut ledger, &call, 1);
    ledger.set_not_dispatched(NonDispatchCause::Stop).unwrap();
    assert_eq!(
        ledger.cancel_unsent(&call, &exchange(1)).unwrap_err(),
        PairError::WrongPhase
    );
    assert!(ledger.next_terminal().is_none());
    assert!(ledger.executing().unwrap().2);
    ledger.observe_gate(received(1, result())).unwrap();
    assert_eq!(
        ledger.cancel_unsent(&call, &exchange(1)).unwrap_err(),
        PairError::WrongPhase
    );
    close(&mut ledger, &call, &scheme, 40);
    assert_eq!(
        ledger.completed(&call).unwrap().unwrap().ended().fate,
        TerminalPairFate::Completed
    );
}
