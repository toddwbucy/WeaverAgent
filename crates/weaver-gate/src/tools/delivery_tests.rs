//! Delivery uses the completed execution's original bound, never another wait.
use super::interrupt_tests::{assert_no_peer_answer, fill_answer_socket};
use super::tests::{assert_wait_delay_consumed, delay_next_wait_until, exchange, pair};
use super::*;
use std::time::{Duration, Instant};

fn answer(deadline: Instant) -> ExecutionAnswer {
    ExecutionAnswer {
        outcome: ToolOutcome::Result {
            content: "contained result".into(),
        },
        containment_deadline: Some(deadline),
    }
}

fn misplaced(peer: &Channel) -> ExchangeId {
    let mut foreign = exchange();
    foreign.ordinal += 1;
    peer.send(&OrganEnvelope {
        exchange: foreign.clone(),
        position: Position::Open,
        payload: Payload::ToolCancel,
    })
    .unwrap();
    foreign
}

fn drain_pressure(peer: &Channel, filled: usize) {
    for _ in 0..filled {
        assert_eq!(peer.recv().unwrap().payload, Payload::ToolCancel);
    }
    assert_no_peer_answer(peer);
}

#[test]
fn an_expired_first_send_is_nondelivery_even_when_the_socket_is_writable() {
    let (gate, peer) = pair();
    let result = send_answer(
        &gate,
        &exchange(),
        answer(Instant::now() - Duration::from_secs(1)),
        &BTreeSet::new(),
    );
    assert!(matches!(result, Err(Unanswered::DeliveryBound {
        exchange: failed, answer: UndeliveredAnswer::ToolAnswer,
    }) if failed == exchange()));
    assert_no_peer_answer(&peer);
}

#[test]
fn an_unsent_refusal_at_containment_finish_names_its_exchange_and_original_bound() {
    let (gate, peer) = pair();
    let filled = fill_answer_socket(&gate);
    let foreign = misplaced(&peer);
    let active = exchange();
    let deadline = Instant::now() + Duration::from_millis(40);
    let mut supervisor = ContainmentSupervisor::new(
        Supervision::new(&gate, &active),
        Ok(Some(KillCause::Unload)),
        deadline,
    );
    supervisor.wait(Duration::ZERO);
    assert!(supervisor.supervision.pending_refusal.is_some());
    let result = supervisor.finish();
    assert!(matches!(result, Err(ShellEnd::DeliveryBound {
        exchange: failed, answer: UndeliveredAnswer::OutOfOrder,
    }) if failed == foreign));
    assert!(Instant::now() >= deadline);
    drain_pressure(&peer, filled);
}

#[test]
fn an_expired_first_refusal_send_is_nondelivery_even_when_the_socket_is_writable() {
    let (gate, peer) = pair();
    let foreign = misplaced(&peer);
    let active = exchange();
    let deadline = Instant::now() - Duration::from_secs(1);
    let mut supervisor = ContainmentSupervisor::new(
        Supervision::new(&gate, &active),
        Ok(Some(KillCause::Unload)),
        deadline,
    );
    // Receiving the misplaced control creates the exact refusal; its first
    // send has not happened, although the opposite direction is writable.
    supervisor.wait(Duration::ZERO);
    assert!(supervisor.supervision.pending_refusal.is_some());
    let result = supervisor.finish();
    assert!(matches!(result, Err(ShellEnd::DeliveryBound {
        exchange: failed, answer: UndeliveredAnswer::OutOfOrder,
    }) if failed == foreign));
    assert_no_peer_answer(&peer);
}

#[test]
fn a_refusal_queued_during_answer_delivery_expires_without_sending_the_tool_answer() {
    let (gate, peer) = pair();
    let filled = fill_answer_socket(&gate);
    let foreign = misplaced(&peer);
    let deadline = Instant::now() + Duration::from_millis(40);
    let result = send_answer(&gate, &exchange(), answer(deadline), &BTreeSet::new());
    assert!(matches!(result, Err(Unanswered::DeliveryBound {
        exchange: failed, answer: UndeliveredAnswer::OutOfOrder,
    }) if failed == foreign));
    assert!(Instant::now() >= deadline);
    drain_pressure(&peer, filled);
}

#[test]
fn observed_peer_teardown_remains_closed_during_answer_delivery_and_pending_refusal() {
    let (gate, peer) = pair();
    drop(peer);
    let result = send_answer(
        &gate,
        &exchange(),
        answer(Instant::now() + Duration::from_secs(1)),
        &BTreeSet::new(),
    );
    assert!(matches!(
        result,
        Err(Unanswered::Channel(ChannelFault::Closed))
    ));

    let (gate, peer) = pair();
    fill_answer_socket(&gate);
    misplaced(&peer);
    let active = exchange();
    let deadline = Instant::now() + Duration::from_secs(1);
    let mut supervisor = ContainmentSupervisor::new(
        Supervision::new(&gate, &active),
        Ok(Some(KillCause::Unload)),
        deadline,
    );
    supervisor.wait(Duration::ZERO);
    assert!(supervisor.supervision.pending_refusal.is_some());
    drop(peer);
    let result = supervisor.finish();
    assert!(matches!(
        result,
        Err(ShellEnd::Channel(ChannelFault::Closed))
    ));
}

#[test]
fn a_terminal_send_rechecks_the_original_bound_after_its_zero_time_wait() {
    let (gate, peer) = pair();
    let deadline = Instant::now() + Duration::from_millis(100);
    delay_next_wait_until(deadline + Duration::from_millis(1));
    let result = send_answer(&gate, &exchange(), answer(deadline), &BTreeSet::new());
    assert_wait_delay_consumed();
    assert!(matches!(result, Err(Unanswered::DeliveryBound {
        exchange: failed, answer: UndeliveredAnswer::ToolAnswer,
    }) if failed == exchange()));
    assert_no_peer_answer(&peer);
}

#[test]
fn a_retained_refusal_rechecks_the_original_bound_after_writable_poll() {
    let (gate, peer) = pair();
    let foreign = misplaced(&peer);
    let active = exchange();
    let deadline = Instant::now() + Duration::from_millis(100);
    let mut supervisor = ContainmentSupervisor::new(
        Supervision::new(&gate, &active),
        Ok(Some(KillCause::Unload)),
        deadline,
    );
    supervisor.wait(Duration::ZERO);
    assert!(supervisor.supervision.pending_refusal.is_some());
    delay_next_wait_until(deadline + Duration::from_millis(1));
    let result = supervisor.finish();
    assert_wait_delay_consumed();
    assert!(matches!(result, Err(ShellEnd::DeliveryBound {
        exchange: failed, answer: UndeliveredAnswer::OutOfOrder,
    }) if failed == foreign));
    assert_no_peer_answer(&peer);
}

#[test]
fn peer_closure_observed_by_a_wait_keeps_its_identity_when_that_wait_crosses_the_bound() {
    let (gate, peer) = pair();
    drop(peer);
    let deadline = Instant::now() + Duration::from_millis(100);
    delay_next_wait_until(deadline + Duration::from_millis(1));
    let result = send_answer(&gate, &exchange(), answer(deadline), &BTreeSet::new());
    assert_wait_delay_consumed();
    assert!(matches!(
        result,
        Err(Unanswered::Channel(ChannelFault::Closed))
    ));

    let (gate, peer) = pair();
    misplaced(&peer);
    let active = exchange();
    let deadline = Instant::now() + Duration::from_millis(100);
    let mut supervisor = ContainmentSupervisor::new(
        Supervision::new(&gate, &active),
        Ok(Some(KillCause::Unload)),
        deadline,
    );
    supervisor.wait(Duration::ZERO);
    assert!(supervisor.supervision.pending_refusal.is_some());
    drop(peer);
    delay_next_wait_until(deadline + Duration::from_millis(1));
    let result = supervisor.finish();
    assert_wait_delay_consumed();
    assert!(matches!(
        result,
        Err(ShellEnd::Channel(ChannelFault::Closed))
    ));
}

/// A failed assertion or a protective mutant must not leave the real child.
/// Its cleanup is bounded even if the host cannot reap a killed process.
struct ReapChild(std::process::Child);

impl Drop for ReapChild {
    fn drop(&mut self) {
        let _ = self.0.kill();
        let end = Instant::now() + Duration::from_secs(2);
        loop {
            match self.0.try_wait() {
                Ok(Some(_)) | Err(_) => break,
                Ok(None) if Instant::now() >= end => break,
                Ok(None) => std::thread::sleep(Duration::from_millis(10)),
            }
        }
    }
}

#[test]
fn a_retained_refusal_cannot_cross_the_original_bound_during_leader_reap() {
    let (gate, peer) = pair();
    let foreign = misplaced(&peer);
    let active = exchange();
    let deadline = Instant::now() + Duration::from_millis(100);
    let mut supervisor = ContainmentSupervisor::new(
        Supervision::new(&gate, &active),
        Ok(Some(KillCause::Unload)),
        deadline,
    );
    supervisor.wait(Duration::ZERO);
    let retained = supervisor.supervision.pending_refusal.clone().unwrap();
    let mut child = ReapChild(
        std::process::Command::new("/bin/sleep")
            .arg("60")
            .stdin(std::process::Stdio::null())
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null())
            .spawn()
            .unwrap(),
    );
    assert!(child.0.try_wait().unwrap().is_none());
    // reap_within progresses the channel even before its first nonblocking
    // child check. Force that actual zero-time wait past the shared deadline.
    delay_next_wait_until(deadline + Duration::from_millis(1));
    assert!(reap_supervised(&mut child.0, deadline, &mut supervisor).is_none());
    assert_wait_delay_consumed();
    assert!(Instant::now() >= deadline);
    assert_eq!(
        supervisor.supervision.pending_refusal.as_ref(),
        Some(&retained)
    );
    assert_no_peer_answer(&peer);
    assert!(matches!(supervisor.finish(), Err(ShellEnd::DeliveryBound {
        exchange: failed, answer: UndeliveredAnswer::OutOfOrder,
    }) if failed == foreign));
}

// The containment constructor requires the original Instant. Its wait accepts
// no optional override, even if production bypasses reap_supervised later.
const _: fn(
    Supervision<'static>,
    Result<Option<KillCause>, ShellEnd>,
    Instant,
) -> ContainmentSupervisor<'static> = ContainmentSupervisor::<'static>::new;
const _: fn(&mut ContainmentSupervisor<'static>, Duration) = ContainmentSupervisor::<'static>::wait;

std::thread_local! {
    static DELAY_PRODUCTION_REAP: std::cell::Cell<bool> = const { std::cell::Cell::new(false) };
    static PRODUCTION_REAP_SEEN: std::cell::Cell<bool> = const { std::cell::Cell::new(false) };
}

/// A timing-only hook in run_in_home, after actual empty-group confirmation.
/// It changes no deadline, channel result, child status or refusal ownership.
pub(super) fn reap_entered(deadline: Instant) {
    DELAY_PRODUCTION_REAP.with(|slot| {
        if slot.replace(false) {
            PRODUCTION_REAP_SEEN.with(|seen| seen.set(true));
            delay_next_wait_until(deadline + Duration::from_millis(1));
        }
    });
}

#[test]
fn a_preexecution_answer_without_a_containment_interval_preserves_control_delivery() {
    let (gate, peer) = pair();
    let foreign = misplaced(&peer);
    let outcome = ToolOutcome::Refused {
        reason: "nothing was started".into(),
    };
    send_answer(
        &gate,
        &exchange(),
        ExecutionAnswer {
            outcome: outcome.clone(),
            containment_deadline: None,
        },
        &BTreeSet::new(),
    )
    .unwrap();
    assert_eq!(
        peer.recv().unwrap(),
        OrganEnvelope {
            exchange: foreign,
            position: Position::Close,
            payload: Payload::Refusal(LifecycleRefusal::OutOfOrder),
        }
    );
    assert_eq!(
        peer.recv().unwrap(),
        OrganEnvelope {
            exchange: exchange(),
            position: Position::Close,
            payload: Payload::ToolAnswer(outcome),
        }
    );
    assert_no_peer_answer(&peer);
}

#[test]
#[ignore = "runs a shell: needs a delegated cgroup scope"]
fn production_leader_reap_cannot_bypass_its_retained_refusal_deadline() {
    use super::tests::{ScratchTools, bare};
    use std::sync::mpsc::sync_channel;
    let scratch = ScratchTools::new();
    let (gate, peer) = pair();
    let (entered, waiting) = sync_channel(1);
    let (release, released) = sync_channel(1);
    let (answer, answered) = sync_channel(1);
    let execution = ToolExecution {
        name: weaver_types::ToolName(SHELL_NAME.into()),
        arguments: serde_json::json!({"command": "printf completed"}).to_string(),
        clock_ms: 2_000,
    };
    std::thread::scope(|scope| {
        let worker = scope.spawn(|| {
            super::interrupt_tests::hold_next_containment(entered, released);
            DELAY_PRODUCTION_REAP.with(|slot| slot.set(true));
            let result = execute_answer_in(
                &execution,
                &gate,
                &exchange(),
                &scratch.tools,
                &bare(),
                None,
            );
            PRODUCTION_REAP_SEEN
                .with(|seen| assert!(seen.get(), "actual run_in_home reap stage was reached"));
            assert_wait_delay_consumed();
            answer.send(result).unwrap();
        });
        waiting
            .recv_timeout(Duration::from_secs(5))
            .expect("actual shell kill reached containment");
        // Ensure the next production empty observation completes in its first
        // iteration: its zero-time channel progress queues the refusal but
        // cannot flush it before the production leader-reap stage.
        assert!(scratch.tools.wait_empty(Duration::from_secs(5)));
        let foreign = misplaced(&peer);
        release.send(()).unwrap();
        let result = answered
            .recv_timeout(cgroup::CONTAINMENT_BOUND + Duration::from_secs(5))
            .expect("one original containment interval ends the execution");
        assert!(matches!(result, Err(Unanswered::DeliveryBound {
            exchange: failed, answer: UndeliveredAnswer::OutOfOrder,
        }) if failed == foreign));
        assert_no_peer_answer(&peer);
        worker.join().unwrap();
        assert_eq!(scratch.tools.populated(), Some(false));
    });
}
