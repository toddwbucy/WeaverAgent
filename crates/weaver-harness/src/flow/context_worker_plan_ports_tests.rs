//! Entered worker and SQLite boot proof, with real root and recorder pressure.
use super::*;
use crate::flow::clock::testing::NonblockingIo;
use crate::flow::clock::{WaitInterest, WaitOutcome};
use crate::flow::context::worker_startup::input::halt::exit::plan_port_tests as failed;
use crate::flow::context::worker_startup::input::management::{
    FoundationManagementProgress,
    plan_refusal::composition_tests::{pressure, prior},
    tests::{Guard, dial, read_packet},
};
use crate::flow::context::worker_startup::tests::Member;
use crate::flow::context::worker_startup::tests::trace_through;
use nix::sys::socket::{MsgFlags, send};
use std::io::Read;
use std::os::fd::{AsRawFd, OwnedFd};
use std::os::unix::net::UnixStream;
use std::path::Path;
use std::time::{Duration, Instant};
use weaver_types::state_flow as s;
use weaver_types::{Cause, ExchangeId, LifecycleAnswer, LifecycleDirective};

#[path = "context_worker_plan_ports_cleanup_tests.rs"]
pub(in crate::flow::context::worker_startup) mod cleanup;

#[path = "context_worker_plan_ports_active_tests.rs"]
mod active;

pub(in crate::flow::context::worker_startup) enum Cleanup {
    Leave(cleanup::Leave),
    Failed(failed::Failure),
}

fn request(input: &FoundationInput<'_>) -> FoundationPlanRequest {
    let seed = input
        .serving
        .owner
        .context
        .open_start
        .as_ref()
        .unwrap()
        .seed_id
        .clone();
    FoundationPlanRequest::WorkReference {
        expected: PlanRef {
            plan_id: s::PlanId {
                seed_id: seed.clone(),
                local_id: 0.into(),
            },
            revision: 1.into(),
        },
        step_id: StepId {
            seed_id: seed.clone(),
            local_id: 0.into(),
        },
        span: SpanRef {
            seed_id: seed,
            local_id: 0.into(),
            revision: 1.into(),
        },
        through: SourcePosition {
            block_id: s::SourceBlockId {
                run: "requested-frontier".into(),
                sequence: 5.into(),
                block: 0.into(),
            },
            turn: None,
        },
    }
}

pub(in crate::flow::context::worker_startup) fn exercise(
    serving: FoundationServing<'_>,
    member: &mut Member,
    reader: &mut UnixStream,
    socket: &Path,
    mode: &str,
) -> Option<Cleanup> {
    if mode.contains("-port-active-") {
        active::exercise(serving, member, reader, socket, mode);
        return None;
    }
    reader.set_nonblocking(true).unwrap();
    let mut guard = Guard {
        input: Some(serving.input()),
        management: None,
    };
    let input = guard.input.as_mut().unwrap();
    let prior = if mode.ends_with("-borrowed") {
        let bound = AbsoluteBound::after(MonotonicIo.now().unwrap(), 7_000_000_000).unwrap();
        input
            .serving
            .owner
            .context
            .arm(PumpDeadline::Decision, bound, &mut MonotonicIo)
            .unwrap();
        Some(bound)
    } else {
        None
    };
    let expected = request(input);
    let request_text = format!("{expected:?}");
    let parent = input
        .serving
        .owner
        .context
        .plan_cache
        .committed
        .as_ref()
        .unwrap()
        .read()
        .recall_sequence();
    let leave = mode.ends_with("-leave");
    let interrupt = leave || mode.contains("-stop");
    let failed = mode.ends_with("-failed");
    let root = mode.ends_with("-root") || mode.ends_with("-root-save");
    if interrupt || failed || root {
        pressure::fill(input.serving.owner.context);
    }
    let FoundationPlanRequest::WorkReference {
        expected,
        step_id,
        span,
        through,
    } = expected
    else {
        unreachable!()
    };
    input
        .work_reference(expected, step_id, span, through, None)
        .unwrap();
    let bound = input
        .serving
        .owner
        .context
        .deadlines
        .get(PumpDeadline::Decision)
        .unwrap();
    if let Some(prior) = prior {
        assert_eq!(bound, prior, "borrowed Decision must not be renewed");
    }
    assert!(input.plan_operation_pending());
    assert!(input.take_plan_operation().is_none());
    if failed {
        let ticket = hold_refusal(input);
        return Some(Cleanup::Failed(failed::begin(
            guard,
            member,
            reader,
            request_text,
            bound,
            ticket,
        )));
    }
    if root {
        let ticket = hold_refusal(input);
        let save = mode
            .ends_with("-root-save")
            .then(|| prior::queue_save(&mut guard, socket));
        prior::refuse(&mut guard, reader, socket, ticket);
        let input = guard.input.as_mut().unwrap();
        let result = input
            .take_plan_operation()
            .expect("prior plan operation settled");
        assert_eq!(format!("{:?}", result.request()), request_text);
        assert!(
            matches!(result.outcome(), FoundationPlanOutcome::Refused(refusal)
            if refusal.case == s::PlanOperationCase::NoLockedPlan)
        );
        assert!(input.take_plan_operation().is_none());
        assert!(
            input
                .serving
                .owner
                .context
                .deadlines
                .get(PumpDeadline::Decision)
                .is_none()
        );
        if let Some((peer, window)) = save {
            assert!(
                input.serving.save.is_some(),
                "original queued save remains owned"
            );
            assert_eq!(
                input
                    .serving
                    .owner
                    .context
                    .deadlines
                    .get(PumpDeadline::SaveQueued),
                Some(window)
            );
            assert!(
                read_packet(&peer).is_none(),
                "no capture boundary was granted"
            );
        }
        return None;
    }
    if mode.ends_with("-busy") {
        let original = request(input);
        let before = format!("{original:?}");
        let error = input
            .begin_plan_operation(
                original,
                Some(TurnKey("unaccepted".into())),
                &mut MonotonicIo,
            )
            .unwrap_err();
        assert_eq!(error.refusal, FoundationPlanAdmissionRefusal::Busy);
        assert_eq!(format!("{:?}", error.request), before);
        assert_eq!(error.turn, Some(TurnKey("unaccepted".into())));
        assert_eq!(
            input
                .serving
                .owner
                .context
                .deadlines
                .get(PumpDeadline::Decision),
            Some(bound)
        );
    }
    let (initial, leave_peer) = if interrupt {
        interrupt_held_refusal(&mut guard, reader, socket, bound, mode)
    } else {
        let limit = Instant::now() + Duration::from_secs(5);
        while input.plan_operation_pending() {
            assert_ne!(
                input.advance(),
                FoundationInputProgress::Stopped,
                "{:?}",
                input.detail()
            );
            assert!(Instant::now() < limit);
        }
        (Vec::new(), None)
    };
    let input = if let Some(owner) = &mut guard.management {
        &mut owner.input
    } else {
        guard.input.as_mut().unwrap()
    };
    assert!(input.serving.owner.context.plan_port_work().is_none());
    assert_eq!(
        input
            .serving
            .owner
            .context
            .deadlines
            .get(PumpDeadline::Decision),
        prior,
        "only this operation's own Decision may retire"
    );
    let result = input
        .serving
        .plan_operation
        .as_ref()
        .unwrap()
        .result
        .as_ref()
        .expect("settled caller account");
    assert_eq!(format!("{:?}", result.request()), request_text);
    if interrupt {
        assert!(matches!(result.outcome(), FoundationPlanOutcome::Cancelled));
    } else {
        assert!(matches!(
            result.outcome(),
            FoundationPlanOutcome::Refused(refusal)
                if refusal.case == s::PlanOperationCase::NoLockedPlan && refusal.read_code.is_none()
        ));
    }
    if !leave {
        assert!(input.take_plan_operation().is_some());
        assert!(input.take_plan_operation().is_none());
    }
    let sequence = input
        .serving
        .owner
        .context
        .barrier
        .latest_recorded()
        .unwrap();
    let rows = trace_through(
        reader,
        initial,
        sequence,
        Instant::now() + Duration::from_secs(5),
    )
    .unwrap();
    let refusals: Vec<_> = rows
        .iter()
        .filter(|row| row["kind"] == "refusal" && row["payload"]["seam"] == "plan_operation")
        .collect();
    assert_eq!(refusals.len(), 1);
    assert_eq!(refusals[0]["causal_parent"], serde_json::json!(parent));
    assert!(refusals[0].get("turn").is_none());
    assert_eq!(refusals[0]["payload"]["operation"], "work_reference");
    assert_eq!(refusals[0]["payload"]["refusal"]["case"], "no_locked_plan");
    assert!(input.serving.owner.context.failure().is_none());
    if prior.is_some() {
        assert_eq!(
            input
                .serving
                .owner
                .context
                .complete_wait(PumpDeadline::Decision),
            prior
        );
    }
    if let Some(peer) = leave_peer {
        let handoff = guard
            .management
            .take()
            .unwrap()
            .rollback_handoff()
            .ok()
            .expect("settled plan account permits actual rollback handoff");
        cleanup::check_account(&handoff, &request_text);
        return Some(Cleanup::Leave(cleanup::Leave {
            handoff,
            peer,
            request: request_text,
            root: socket.parent().unwrap().to_path_buf(),
        }));
    }
    None
}

fn hold_refusal(input: &mut FoundationInput<'_>) -> u64 {
    let interrupt = AbsoluteBound::after(MonotonicIo.now().unwrap(), 100_000_000).unwrap();
    input
        .serving
        .owner
        .context
        .arm(PumpDeadline::Label, interrupt, &mut MonotonicIo)
        .unwrap();
    assert!(matches!(
        input.advance_plan_operation(None, &mut MonotonicIo),
        Err(RunFailure::Wait { key: PumpDeadline::Label, bound }) if bound == interrupt
    ));
    input
        .serving
        .owner
        .context
        .complete_wait(PumpDeadline::Label)
        .unwrap();
    input.serving.owner.context.pending_author().unwrap().ticket
}

fn interrupt_held_refusal(
    guard: &mut Guard<'_>,
    reader: &mut UnixStream,
    socket: &Path,
    original: AbsoluteBound,
    mode: &str,
) -> (Vec<u8>, Option<OwnedFd>) {
    let leave = mode.ends_with("-leave");
    let ticket = hold_refusal(guard.input.as_mut().unwrap());
    let peer = dial(socket);
    let bytes = serde_json::to_vec(&OrganEnvelope {
        exchange: ExchangeId {
            opener: Opener::Admin,
            ordinal: 411,
        },
        position: Position::Open,
        payload: Payload::Directive(if leave {
            LifecycleDirective::Leave {
                cause: Cause { uid: 0 },
                forced: false,
                rollback: true,
            }
        } else {
            LifecycleDirective::Stop {
                cause: Cause { uid: 0 },
            }
        }),
    })
    .unwrap();
    assert_eq!(
        send(peer.as_raw_fd(), &bytes, MsgFlags::MSG_NOSIGNAL).unwrap(),
        bytes.len()
    );
    let limit = Instant::now() + Duration::from_secs(5);
    while guard
        .input
        .as_mut()
        .unwrap()
        .advance_with_io(&mut NonblockingIo { waits: 0 })
        != FoundationInputProgress::ManagementPending
    {
        assert!(Instant::now() < limit);
    }
    guard.management = Some(guard.input.take().unwrap().management().ok().unwrap());
    let owner = guard.management.as_mut().unwrap();
    assert!(read_packet(&peer).is_none());
    assert!(owner.input.take_plan_operation().is_none());
    assert_eq!(
        owner
            .input
            .serving
            .owner
            .context
            .pending_author()
            .unwrap()
            .ticket,
        ticket
    );
    assert_eq!(
        owner
            .input
            .serving
            .owner
            .context
            .deadlines
            .get(PumpDeadline::Decision),
        Some(original)
    );
    // The author waits inside this call. A nonblocking transport does not make
    // that nested wait an incremental reducer. Release the actual sink only
    // after its wait proves no Stop answer escaped the owed refusal.
    let mut io = ReleaseTrace {
        reader,
        peer: &peer,
        bound: original,
        bytes: vec![],
        waited: false,
    };
    let wanted = if leave {
        FoundationManagementProgress::DirectivePending
    } else {
        FoundationManagementProgress::Answered
    };
    while owner.progress() != wanted {
        assert_ne!(
            owner.advance_with_io(&mut io),
            FoundationManagementProgress::Stopped,
            "{:?}",
            owner.detail()
        );
        assert!(Instant::now() < limit);
    }
    assert!(io.waited, "Stop must settle the actual full-recorder wait");
    let initial = io.bytes;
    let context = &owner.input.serving.owner.context;
    assert_eq!(
        context
            .barrier
            .recorded_author_ticket(context.barrier.latest_recorded().unwrap()),
        Some(ticket),
        "the original refusal ticket records before Stop answers"
    );
    if leave {
        assert!(
            read_packet(&peer).is_none(),
            "Leave still owes actual cleanup"
        );
        return (initial, Some(peer));
    }
    let answer: OrganEnvelope = serde_json::from_slice(&read_packet(&peer).unwrap()).unwrap();
    assert_eq!(answer.payload, Payload::Answer(LifecycleAnswer::AtRest));
    guard.input = Some(guard.management.take().unwrap().finish().ok().unwrap());
    (initial, None)
}

pub(in crate::flow::context::worker_startup) fn check_pending(
    work: &PendingPlanOperation,
    bound: AbsoluteBound,
) {
    assert_eq!(work.owned_decision, Some(bound));
    assert!(
        work.result.is_none(),
        "failed operation grants no completion"
    );
}

struct ReleaseTrace<'a> {
    reader: &'a mut UnixStream,
    peer: &'a OwnedFd,
    bound: AbsoluteBound,
    bytes: Vec<u8>,
    waited: bool,
}

impl PumpIo for ReleaseTrace<'_> {
    fn now(&mut self) -> std::io::Result<u64> {
        MonotonicIo.now()
    }

    fn wait(
        &mut self,
        interests: &[WaitInterest<'_>],
        deadline: Option<u64>,
    ) -> std::io::Result<WaitOutcome> {
        if !self.waited {
            assert!(
                read_packet(self.peer).is_none(),
                "Stop answered before Recorded"
            );
            assert!(
                deadline.is_some_and(|deadline| deadline <= self.bound.deadline_ns()),
                "the refusal wait lost or renewed its original Decision"
            );
            self.waited = true;
        }
        match self.reader.read_to_end(&mut self.bytes) {
            Ok(_) => (),
            Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => (),
            Err(error) => return Err(error),
        }
        MonotonicIo.wait(interests, deadline)
    }
}
