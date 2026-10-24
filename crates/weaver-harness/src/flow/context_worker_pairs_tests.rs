//! Actual worker/gate/checker/member joins with scripted model and store peers.
use super::super::tests::{FAMILY, selection, sequence, stop};
use super::*;
use crate::flow::clock::testing::FakePumpIo;
use crate::flow::context::worker_startup::input::generation::tests::{step, until};
use crate::flow::context::worker_startup::input::management::tests::{Guard, trace};
use crate::flow::context::worker_startup::tests::{Member, worker};
use std::os::unix::net::UnixStream;
use std::path::Path;
use std::time::{Duration, Instant};
use weaver_types::state_flow as s;

fn recall_reads(member: &Member) -> Vec<&str> {
    member
        .requests
        .iter()
        .filter_map(|r| r.strip_prefix("recall:"))
        .collect()
}
fn assert_close(rows: &[serde_json::Value], count: usize) {
    let ends: Vec<_> = rows
        .iter()
        .filter(|r| r["kind"] == "tool.call.completed")
        .collect();
    assert_eq!(ends.len(), count);
    let close = rows.iter().find(|r| r["kind"] == "turn.closed").unwrap();
    assert!(ends.iter().all(|r| sequence(r) < sequence(close)));
    assert_eq!(
        rows.iter().filter(|r| r["kind"] == "turn.closed").count(),
        1
    );
}
pub(in crate::flow::context) fn exercise(
    mut guard: Guard<'_>,
    member: &mut Member,
    reader: &mut UnixStream,
    clock: &mut FakePumpIo,
    socket: &Path,
    root: &Path,
    mode: &str,
) {
    let input = guard.input.as_mut().unwrap();
    assert_eq!(input.begin_pairs(), Err(FoundationInputProgress::Generated));
    if mode.contains("actual-worker") {
        return actual_worker(guard, member, reader, mode);
    }
    input.repair(&FAMILY).unwrap();
    until(
        input,
        member,
        clock,
        mode,
        FoundationInputProgress::Effective,
    );
    let requests = input.effective_emission().unwrap().requests().to_vec();
    assert!(
        requests
            .iter()
            .all(|p| input.pair_outcome(&p.call_id).is_none())
    );
    let mut acts = selection(input.effective_emission().unwrap());
    if mode.contains("omit-first") || mode.contains("mixed") {
        let first = &requests[0].call_id;
        acts.retain(|act| match act {
            SortAct::Tool { call_id, .. } | SortAct::Recall { call_id, .. } => call_id != first,
            _ => true,
        });
    }
    if mode.contains("barred") {
        acts.clear();
    }
    // Reversed outbound acts do not reorder conforming pair execution.
    if mode.contains("reverse") {
        acts.reverse();
    }
    input.sort_outbound(acts.clone()).unwrap();
    assert_eq!(input.begin_pairs(), Err(FoundationInputProgress::Pending));
    if mode.contains("held-statement") {
        for _ in 0..32 {
            assert_eq!(
                step(input, member, clock, mode),
                FoundationInputProgress::Pending
            );
        }
        assert!(member.held_file.is_some());
        assert!(input.ready_pair().is_none());
        assert!(input.begin_pairs().is_err());
        assert!(recall_reads(member).is_empty());
        assert!(
            !member
                .filings
                .iter()
                .any(|f| matches!(f, s::StateFrame::FilePair { .. }))
        );
        member.hold_kind = None;
    }
    until(input, member, clock, mode, FoundationInputProgress::Sorted);
    assert!(recall_reads(member).is_empty());
    assert!(
        !member
            .filings
            .iter()
            .any(|f| matches!(f, s::StateFrame::FilePair { .. }))
    );
    input.begin_pairs().unwrap();
    assert_eq!(input.begin_pairs(), Err(FoundationInputProgress::Pending));
    if mode.contains("stop-begin") {
        stop(&mut guard, member, clock, mode, socket, root);
        assert!(recall_reads(member).is_empty());
        assert_close(&trace(reader), requests.len());
        return;
    }
    if mode.contains("held-opening") {
        member.hold_kind = Some("tool.call.started");
    }
    if mode.contains("held-opening") {
        for _ in 0..100 {
            assert_eq!(
                step(input, member, clock, mode),
                FoundationInputProgress::Pending
            );
            if member.held_file.is_some() {
                break;
            }
        }
        assert!(member.held_file.is_some());
        assert!(input.ready_pair().is_none());
        assert!(input.recall(requests[0].call_id.clone()).is_err());
        assert!(recall_reads(member).is_empty());
        member.hold_kind = None;
    }
    if requests.is_empty() || mode.contains("barred") {
        until(
            input,
            member,
            clock,
            mode,
            FoundationInputProgress::PairsComplete,
        );
    } else {
        let first = usize::from(mode.contains("omit-first") || mode.contains("mixed"));
        for (index, request) in requests.iter().enumerate().skip(first) {
            let input = guard.input.as_mut().unwrap();
            until(
                input,
                member,
                clock,
                mode,
                FoundationInputProgress::PairReady,
            );
            assert_eq!(input.ready_pair(), Some(request));
            let before = recall_reads(member).len();
            for _ in 0..4 {
                assert_eq!(
                    step(input, member, clock, mode),
                    FoundationInputProgress::PairReady
                );
            }
            assert_eq!(
                recall_reads(member).len(),
                before,
                "read requires the worker election"
            );
            let mut foreign = request.call_id.clone();
            foreign.call.0 += 100;
            assert_eq!(input.recall(foreign.clone()), Err(foreign));
            if request.lane == PairLane::External {
                assert!(input.recall(request.call_id.clone()).is_err());
                assert_eq!(input.ready_pair(), Some(request));
                stop(&mut guard, member, clock, mode, socket, root);
                assert!(recall_reads(member).is_empty());
                assert_close(&trace(reader), requests.len());
                return;
            }
            if index == first && mode.contains("stop-ready") {
                stop(&mut guard, member, clock, mode, socket, root);
                assert!(recall_reads(member).is_empty());
                let rows = trace(reader);
                assert_close(&rows, requests.len());
                if mode.contains("omit-first") {
                    let ends: Vec<_> = rows
                        .iter()
                        .filter(|r| r["kind"] == "tool.call.completed")
                        .collect();
                    assert_eq!(
                        ends[0]["payload"]["reason"]["not_dispatched"]["cause"],
                        "plan_break"
                    );
                    assert_eq!(
                        ends[1]["payload"]["reason"]["not_dispatched"]["cause"],
                        "stop"
                    );
                }
                return;
            }
            if index == first && (mode.contains("held-read") || mode.contains("stop-read")) {
                member.hold_control = Some("read");
            }
            if index == first && mode.contains("held-release") {
                member.hold_control = Some("release");
            }
            if index == first && (mode.contains("held-terminal") || mode.contains("stop-terminal"))
            {
                member.hold_kind = Some("tool.call.completed");
            }
            input.recall(request.call_id.clone()).unwrap();
            assert!(input.recall(request.call_id.clone()).is_err());
            if index == first && mode.contains("stop-elected") {
                stop(&mut guard, member, clock, mode, socket, root);
                assert!(recall_reads(member).is_empty());
                assert_close(&trace(reader), requests.len());
                return;
            }
            if member.hold_control.is_some() || member.hold_kind.is_some() {
                for _ in 0..1000 {
                    assert_eq!(
                        step(input, member, clock, mode),
                        FoundationInputProgress::Pending,
                        "{:?}",
                        input.detail()
                    );
                    if member.held_control.is_some() || member.held_file.is_some() {
                        break;
                    }
                }
                assert!(member.held_control.is_some() || member.held_file.is_some());
                if member.held_control.is_some() {
                    assert!(
                        input
                            .serving
                            .owner
                            .context
                            .deadlines
                            .get(PumpDeadline::Decision)
                            .is_none()
                    );
                    assert!(
                        input
                            .serving
                            .owner
                            .context
                            .deadlines
                            .get(PumpDeadline::StateControl)
                            .is_some()
                    );
                }
                assert!(input.pair_outcome(&request.call_id).is_none());
                assert!(input.ready_pair().is_none());
                assert_eq!(recall_reads(member).len(), before + 1);
                for _ in 0..4 {
                    assert_eq!(
                        step(input, member, clock, mode),
                        FoundationInputProgress::Pending
                    );
                }
                if mode.contains("expired") {
                    let bound = input
                        .serving
                        .owner
                        .context
                        .deadlines
                        .get(PumpDeadline::StateControl)
                        .unwrap();
                    clock.advance_to(bound.deadline_ns()).unwrap();
                    assert_eq!(
                        step(input, member, clock, mode),
                        FoundationInputProgress::Stopped
                    );
                    let failure = input.detail().unwrap();
                    assert!(input.serving.owner.context.read.is_some());
                    assert!(input.serving.owner.context.pair_work.is_some());
                    assert!(input.pair_outcome(&request.call_id).is_none());
                    assert_eq!(
                        step(input, member, clock, mode),
                        FoundationInputProgress::Stopped
                    );
                    assert_eq!(input.detail().as_deref(), Some(failure.as_str()));
                    assert_eq!(recall_reads(member).len(), 1);
                    assert!(!root.join("closed.json").exists());
                    return;
                }
                if mode.contains("stop-read") || mode.contains("stop-terminal") {
                    stop(&mut guard, member, clock, mode, socket, root);
                    let rows = trace(reader);
                    assert_close(&rows, requests.len());
                    let ends: Vec<_> = rows
                        .iter()
                        .filter(|r| r["kind"] == "tool.call.completed")
                        .collect();
                    assert_eq!(ends[0]["payload"]["fate"], "completed");
                    assert_eq!(
                        ends[1]["payload"]["reason"]["not_dispatched"]["cause"],
                        "stop"
                    );
                    assert_eq!(recall_reads(member).len(), 1);
                    return;
                }
                member.hold_control = None;
                member.hold_kind = None;
            }
        }
        until(
            guard.input.as_mut().unwrap(),
            member,
            clock,
            mode,
            FoundationInputProgress::PairsComplete,
        );
    }
    let input = guard.input.as_ref().unwrap();
    assert!(input.ready_pair().is_none());
    for (index, request) in requests.iter().enumerate() {
        let end = input
            .pair_outcome(&request.call_id)
            .expect("all actual terminal proofs retained");
        assert_eq!(end.execution, None);
        if mode.contains("barred")
            || (index == 0 && (mode.contains("omit-first") || mode.contains("mixed")))
        {
            assert_eq!(
                end.reason,
                Some(s::PairFailure::NotDispatched {
                    cause: NonDispatchCause::PlanBreak
                })
            );
            assert!(end.outcome.is_none());
        } else if mode.contains("read-refused") {
            assert_eq!(end.fate, s::TerminalPairFate::Completed);
            assert_eq!(end.outcome, Some(weaver_types::ToolOutcome::Refused { reason: "{\"code\":\"result_too_large\",\"detail\":\"whole history exceeds the fixture bound\"}".into() }));
        } else {
            assert_eq!(end.fate, s::TerminalPairFate::Completed);
            assert_eq!(
                end.outcome,
                Some(weaver_types::ToolOutcome::Result {
                    content: "{ \"entries\" : { \"entries\" : [] } }".into()
                })
            );
        }
    }
    let work = input.engine.as_ref().unwrap().emission.as_ref().unwrap();
    assert_eq!(&work.selected.as_ref().unwrap().1, &acts);
    assert!(work.sort_record.is_some());
    assert_eq!(
        work.execution
            .as_ref()
            .unwrap()
            .effective
            .as_ref()
            .unwrap()
            .requests(),
        &requests
    );
    if mode.contains("stop-complete") {
        stop(&mut guard, member, clock, mode, socket, root);
        assert_close(&trace(reader), requests.len());
        return;
    }
    let rows = trace(reader);
    assert!(
        rows.iter()
            .all(|r| r["kind"] != "turn.closed" && r["kind"] != "message.tool_result")
    );
    let starts: Vec<_> = rows
        .iter()
        .filter(|r| r["kind"] == "tool.call.started")
        .collect();
    let ends: Vec<_> = rows
        .iter()
        .filter(|r| r["kind"] == "tool.call.completed")
        .collect();
    assert_eq!(starts.len(), requests.len());
    assert_eq!(ends.len(), requests.len());
    for (index, request) in requests.iter().enumerate() {
        assert_eq!(
            starts[index]["payload"]["call_id"],
            serde_json::to_value(&request.call_id).unwrap()
        );
        assert_eq!(
            starts[index]["payload"]["statement"],
            serde_json::to_value(&request.statement).unwrap()
        );
        assert!(sequence(starts[index]) < sequence(ends[index]));
        if index > 0 {
            assert!(sequence(ends[index - 1]) < sequence(starts[index]));
        }
    }
    assert!(!root.join("closed.json").exists());
}

fn actual_worker(mut guard: Guard<'_>, member: &mut Member, reader: &mut UnixStream, mode: &str) {
    let done = std::sync::atomic::AtomicBool::new(false);
    guard.input = Some(std::thread::scope(|scope| {
        let responder = scope.spawn(|| {
            let end = Instant::now() + Duration::from_secs(8);
            while !done.load(std::sync::atomic::Ordering::SeqCst) && Instant::now() < end {
                member.step(MonotonicIo.now().unwrap(), mode);
                std::thread::sleep(Duration::from_millis(1));
            }
        });
        let input = worker::repair(guard.input.take().unwrap(), &FAMILY);
        assert_eq!(
            input.progress(),
            FoundationInputProgress::Effective,
            "{:?}",
            input.detail()
        );
        let requests = input.effective_emission().unwrap().requests().to_vec();
        let acts = selection(input.effective_emission().unwrap());
        let input = worker::outbound(input, acts);
        let mut input = worker::pairs(input);
        for request in requests {
            assert_eq!(
                input.progress(),
                FoundationInputProgress::PairReady,
                "{:?}",
                input.detail()
            );
            assert_eq!(input.ready_pair(), Some(&request));
            input = worker::recall(input, request.call_id.clone());
            assert!(input.pair_outcome(&request.call_id).is_some());
        }
        done.store(true, std::sync::atomic::Ordering::SeqCst);
        responder.join().unwrap();
        input
    }));
    assert_eq!(
        guard.input.as_ref().unwrap().progress(),
        FoundationInputProgress::PairsComplete
    );
    assert_eq!(recall_reads(member), ["archive", "ledger"]);
    let rows = trace(reader);
    assert_eq!(
        rows.iter()
            .filter(|r| r["kind"] == "tool.call.completed")
            .count(),
        2
    );
    assert!(
        !rows
            .iter()
            .any(|r| r["kind"] == "turn.closed" || r["kind"] == "message.tool_result")
    );
}
