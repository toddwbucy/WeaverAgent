//! Actual entered worker and root socket. The member and model remain fixtures.
use super::*;
use crate::flow::clock::testing::{NoReadyIo, NonblockingIo};
use crate::flow::context::worker_startup::input::management::tests::{Guard, dial, read_packet};
use crate::flow::context::worker_startup::tests::{Member, trace_through};
use nix::sys::socket::{MsgFlags, send};
use std::os::fd::{AsRawFd, OwnedFd};
use std::os::unix::net::UnixStream;
use std::path::Path;
use std::time::{Duration, Instant};
use weaver_types::state_flow::Digest;
use weaver_types::{Cause, LifecycleAsk, Opener, RefusalRecord};

#[path = "context_worker_plan_refusal_pressure_tests.rs"]
pub(in crate::flow::context::worker_startup) mod pressure;

#[path = "context_worker_plan_refusal_prior_tests.rs"]
pub(in crate::flow::context::worker_startup) mod prior;

fn failed_handoff(
    guard: &mut Guard<'_>,
    clock: &mut impl PumpIo,
    bytes: &[u8],
    bound: AbsoluteBound,
    ticket: Option<u64>,
) {
    use crate::flow::context::worker_startup::input::halt::{FoundationHaltProgress, exit};
    let owner = guard.management.take().unwrap();
    let first = owner.input.serving.owner.context.failure().unwrap().clone();
    let mut halt = match owner.halt() {
        Ok(halt) => halt,
        Err(owner) => {
            guard.management = Some(owner);
            panic!("failed management cannot abandon refusal custody");
        }
    };
    let watch = Instant::now() + Duration::from_secs(3);
    while halt.advance_with_io(clock) != FoundationHaltProgress::Prepared {
        assert!(
            Instant::now() < watch,
            "failed refusal account never reached handoff"
        );
    }
    assert_eq!(
        halt.test_input().serving.owner.context.failure(),
        Some(&first)
    );
    let handoff = halt
        .handoff()
        .ok()
        .expect("terminal failed management must hand off");
    exit::tests::check_management_handoff(handoff, &first, |failed| {
        assert_eq!(failed._declined_plan.as_ref().unwrap().wire(), bytes);
        let retained = failed._plan_refusal.as_ref().unwrap();
        assert_eq!(retained.author.ticket(), ticket);
        assert_eq!(retained.original.is_some(), ticket.is_none());
        assert_eq!(retained.bound, bound);
        assert!(failed._answer.is_some());
    });
}

struct SendClockFailure<'a> {
    peer: &'a OwnedFd,
}
impl PumpIo for SendClockFailure<'_> {
    fn now(&mut self) -> std::io::Result<u64> {
        let mut byte = [0];
        match nix::sys::socket::recv(
            self.peer.as_raw_fd(),
            &mut byte,
            MsgFlags::MSG_PEEK | MsgFlags::MSG_DONTWAIT,
        ) {
            Ok(n) if n > 0 => Err(std::io::Error::other(
                "fixture fails the post-send clock reading",
            )),
            Err(nix::errno::Errno::EAGAIN) => MonotonicIo.now(),
            other => panic!("peek delivered answer: {other:?}"),
        }
    }
    fn wait(
        &mut self,
        interests: &[crate::flow::clock::WaitInterest<'_>],
        deadline: Option<u64>,
    ) -> std::io::Result<crate::flow::clock::WaitOutcome> {
        NonblockingIo { waits: 0 }.wait(interests, deadline)
    }
}

fn request(socket: &Path, cause: u32) -> (OwnedFd, Vec<u8>) {
    let peer = dial(socket);
    // Admission refusal must not parse the document or require a matching digest.
    let packet = OrganEnvelope {
        exchange: ExchangeId {
            opener: Opener::Admin,
            ordinal: 611,
        },
        position: Position::Open,
        payload: Payload::Directive(LifecycleDirective::PlanUpdate {
            cause: Cause { uid: cause },
            document_sha256: Digest::new("a".repeat(64)).unwrap(),
            document: "  { unparsed plan document }\n".into(),
        }),
    };
    let bytes = serde_json::to_vec_pretty(&packet).unwrap();
    assert_eq!(
        send(peer.as_raw_fd(), &bytes, MsgFlags::MSG_NOSIGNAL).unwrap(),
        bytes.len()
    );
    (peer, bytes)
}

pub(in crate::flow::context::worker_startup) fn exercise(
    serving: FoundationServing<'_>,
    member: &mut Member,
    reader: &mut UnixStream,
    socket: &Path,
    mode: &str,
) {
    if mode.ends_with("-root-author") || mode.ends_with("-queued-save-author") {
        return prior::management_author(
            serving,
            member,
            reader,
            socket,
            mode.ends_with("-queued-save-author"),
        );
    }
    if mode.ends_with("-quiesce-author") {
        return prior::quiesce(serving, member, reader, socket);
    }
    if mode.ends_with("-source-author") || mode.ends_with("-task-author") {
        return prior::turn(
            serving,
            member,
            reader,
            socket,
            mode.ends_with("-task-author"),
            mode.contains("-queued-"),
        );
    }
    reader.set_nonblocking(true).unwrap();
    let earlier_expiry = mode.ends_with("-earlier-expiry");
    let inner = mode.ends_with("-inner") || earlier_expiry;
    let eof = mode == "input-worker-plan-refusal-eof";
    let expiry = mode.ends_with("-delivery-expiry");
    let sent_expiry = mode.ends_with("-sent-expiry");
    let accounting_expiry = mode.ends_with("-accounting-expiry") || earlier_expiry;
    let sent_clock = mode.ends_with("-sent-clock");
    let mut guard = Guard {
        input: Some(serving.input()),
        management: None,
    };
    let (peer, bytes) = request(socket, if inner { 0 } else { 42 });
    let mut io = NonblockingIo { waits: 0 };
    let watch = Instant::now() + Duration::from_secs(5);
    while guard.input.as_mut().unwrap().advance_with_io(&mut io)
        != FoundationInputProgress::ManagementPending
    {
        member.step_live("ready");
        assert!(
            Instant::now() < watch,
            "root refusal connection not accepted"
        );
    }
    guard.management = Some(guard.input.take().unwrap().management().ok().unwrap());
    loop {
        let owner = guard.management.as_mut().unwrap();
        let progress = owner.advance_with_io(&mut io);
        assert_ne!(
            progress,
            FoundationManagementProgress::Stopped,
            "{:?}",
            owner.detail()
        );
        if progress == FoundationManagementProgress::DirectivePending {
            break;
        }
        member.step_live("ready");
        assert!(Instant::now() < watch, "root opener not retained");
    }
    let owner = guard.management.as_mut().unwrap();
    let heard = owner.heard_ns.unwrap();
    let old_summary = owner
        .input
        .serving
        .owner
        .context
        .committed_plan_summary()
        .unwrap()
        .cloned();
    let original_stage = owner.input.stage;
    let earlier = inner.then(|| {
        // The outer at-rest predicate still holds, but the primitive rejects a
        // competing retained interval. The rejected document grants no work.
        let context = &mut owner.input.serving.owner.context;
        let bound = AbsoluteBound::after(
            heard,
            if earlier_expiry {
                50_000_000
            } else {
                30_000_000_000
            },
        )
        .unwrap();
        context.arm(PumpDeadline::Decision, bound, &mut io).unwrap();
        bound
    });
    if eof {
        nix::sys::socket::shutdown(peer.as_raw_fd(), nix::sys::socket::Shutdown::Both).unwrap();
    }
    guard.management = Some(
        guard
            .management
            .take()
            .unwrap()
            .queue_plan_update_with_io(&mut io)
            .err()
            .expect("request must be refused"),
    );
    let owner = guard.management.as_mut().unwrap();
    assert_eq!(owner.progress(), FoundationManagementProgress::Pending);
    assert_eq!(owner.declined_plan.as_ref().unwrap().wire(), bytes);
    assert!(owner.message.is_none());
    let bound = owner.plan_refusal.as_ref().unwrap().bound;
    assert_eq!(bound.began_ns(), heard);
    let duration = owner
        .input
        .serving
        .owner
        .context
        .load_duty
        .as_ref()
        .unwrap()
        .instruction
        .decision_wait
        .elapsed_ns
        .0;
    assert_eq!(bound.deadline_ns(), heard + duration);
    assert_eq!(
        owner
            .input
            .serving
            .owner
            .context
            .deadlines
            .get(PumpDeadline::PlanRefusal),
        Some(bound)
    );
    assert_eq!(
        owner
            .input
            .serving
            .owner
            .context
            .deadlines
            .get(PumpDeadline::Decision),
        earlier
    );
    assert!(read_packet(&peer).is_none(), "answer preceded accounting");
    if eof {
        // Observe the actual peer EOF before admitting the trace claim. A
        // single reduction that both notices EOF and records would miss a
        // premature-completion guard on the next entry.
        owner.pump(&mut io, false).unwrap();
        assert!(owner.peer_unavailable);
        let record = owner.plan_refusal.as_ref().unwrap();
        assert!(record.original.is_some());
        assert!(record.author.ticket().is_none());
        assert!(!record.complete());
    }
    if mode.ends_with("-trace-eof")
        || mode.ends_with("-pending-expiry")
        || mode.ends_with("-recorded-expiry")
    {
        pressure::exercise(&mut guard, &peer, reader, &bytes, mode);
        return;
    }
    if accounting_expiry {
        let expiring = if earlier_expiry {
            earlier.unwrap()
        } else {
            bound
        };
        let mut expired = NoReadyIo {
            now: expiring.deadline_ns(),
            waits: 0,
        };
        assert_eq!(
            owner.advance_with_io(&mut expired),
            FoundationManagementProgress::Stopped
        );
        assert!(matches!(
            owner.input.serving.owner.context.failure(),
            Some(RunFailure::Flow {
                reason: FlowHaltReason::DecisionBound,
                ..
            })
        ));
        let wait = owner.input.serving.owner.context.halt_wait().unwrap();
        assert_eq!(wait.began_ns.0, expiring.began_ns());
        assert_eq!(wait.deadline_ns.0, expiring.deadline_ns());
        if earlier_expiry {
            assert!(expiring.deadline_ns() < bound.deadline_ns());
            assert!(matches!(
                owner.input.serving.owner.context.failure(),
                Some(RunFailure::Flow {
                    point: HaltPoint::Barrier,
                    ..
                })
            ));
        }
        assert!(owner.send.is_none());
        assert!(read_packet(&peer).is_none());
        let first = owner.input.serving.owner.context.failure().cloned();
        owner.input.serving.owner.context.fail(
            &mut expired,
            FlowHaltReason::ProtocolFailed,
            HaltPoint::Teardown,
            "later failure cannot replace refusal expiry",
        );
        assert_eq!(owner.input.serving.owner.context.failure(), first.as_ref());
        failed_handoff(&mut guard, &mut expired, &bytes, bound, None);
        return;
    }
    if expiry {
        let mut filled = 0;
        let filler = [b'x'; 65536];
        for packet in [&filler[..], &filler[..1]] {
            loop {
                match send(
                    owner.peer.fd().as_raw_fd(),
                    packet,
                    MsgFlags::MSG_DONTWAIT | MsgFlags::MSG_NOSIGNAL,
                ) {
                    Ok(n) => {
                        assert_eq!(n, packet.len());
                        filled += 1;
                    }
                    Err(nix::errno::Errno::EAGAIN) => break,
                    other => panic!("fill answer socket: {other:?}"),
                }
            }
        }
        assert!(filled > 0);
    }
    while !guard
        .management
        .as_ref()
        .unwrap()
        .plan_refusal
        .as_ref()
        .unwrap()
        .complete()
    {
        let owner = guard.management.as_mut().unwrap();
        assert!(owner.send.is_none(), "send queued before Recorded");
        assert_eq!(
            owner.advance_with_io(&mut io),
            FoundationManagementProgress::Pending,
            "{:?}",
            owner.detail()
        );
        member.step_live("ready");
        assert!(Instant::now() < watch, "refusal never Recorded");
    }
    let owner = guard.management.as_mut().unwrap();
    let sequence = owner.plan_refusal.as_ref().unwrap().sequence.unwrap();
    assert_eq!(
        owner.plan_refusal.as_ref().unwrap().author.sequence(),
        Some(sequence)
    );
    assert!(
        owner
            .input
            .serving
            .owner
            .context
            .serving_observation_author
            .is_none(),
        "exact claim must consume its completion"
    );
    if sent_clock {
        let mut clock = SendClockFailure { peer: &peer };
        while owner.advance_with_io(&mut clock) == FoundationManagementProgress::Pending {
            assert!(Instant::now() < watch);
        }
        assert_eq!(owner.progress(), FoundationManagementProgress::Stopped);
        assert!(matches!(
            owner.input.serving.owner.context.failure(),
            Some(RunFailure::Flow {
                reason: FlowHaltReason::TraceFailed,
                ..
            })
        ));
        let send = owner.send.as_ref().unwrap();
        assert!(send.sent());
        assert!(send.sent_at().is_none());
        let answer: OrganEnvelope = serde_json::from_slice(&read_packet(&peer).unwrap()).unwrap();
        assert_eq!(
            answer.payload,
            Payload::Refusal(LifecycleRefusal::Unauthorized)
        );
        assert_eq!(
            owner.advance_with_io(&mut clock),
            FoundationManagementProgress::Stopped
        );
        assert!(
            read_packet(&peer).is_none(),
            "a sent answer must not be retried"
        );
        return;
    }
    if sent_expiry {
        while !owner.send.as_ref().is_some_and(OrganSend::sent) {
            assert_eq!(
                owner.advance_with_io(&mut io),
                FoundationManagementProgress::Pending
            );
            assert!(Instant::now() < watch);
        }
        assert!(owner.send.as_ref().unwrap().sent_at().unwrap() < bound.deadline_ns());
        let mut expired = NoReadyIo {
            now: bound.deadline_ns(),
            waits: 0,
        };
        for _ in 0..5 {
            if owner.advance_with_io(&mut expired) != FoundationManagementProgress::Pending {
                break;
            }
        }
        assert_eq!(
            owner.progress(),
            FoundationManagementProgress::Answered,
            "a timely sent answer cannot become dropped at the next reduction"
        );
    }
    if expiry {
        assert_eq!(
            owner.advance_with_io(&mut io),
            FoundationManagementProgress::Pending
        );
        assert!(
            !owner.send.as_ref().unwrap().sent(),
            "actual socket pressure must block the refusal answer"
        );
        let mut expired = NoReadyIo {
            now: bound.deadline_ns(),
            waits: 0,
        };
        for _ in 0..5 {
            if owner.advance_with_io(&mut expired) == FoundationManagementProgress::Dropped {
                break;
            }
        }
        assert_eq!(owner.progress(), FoundationManagementProgress::Dropped);
        assert!(!owner.send.as_ref().is_some_and(OrganSend::sent));
    } else {
        while owner.progress() == FoundationManagementProgress::Pending {
            owner.advance_with_io(&mut io);
            assert!(Instant::now() < watch, "refusal delivery did not settle");
        }
        assert_eq!(
            owner.progress(),
            if eof {
                FoundationManagementProgress::Dropped
            } else {
                FoundationManagementProgress::Answered
            }
        );
    }
    assert!(
        owner.input.serving.owner.context.failure().is_none(),
        "healthy refusal must preserve the run"
    );
    assert!(
        owner.input.stage == original_stage,
        "refusal changed the suspended input stage"
    );
    assert!(!owner.input.plan_pending());
    assert!(owner.input.serving.owner.context.root_plan_work().is_none());
    assert_eq!(
        owner
            .input
            .serving
            .owner
            .context
            .committed_plan_summary()
            .unwrap()
            .cloned(),
        old_summary
    );
    let expected = if inner {
        LifecycleRefusal::OutOfOrder
    } else {
        LifecycleRefusal::Unauthorized
    };
    if !eof && !expiry {
        let response: OrganEnvelope =
            serde_json::from_slice(&read_packet(&peer).expect("one refusal answer")).unwrap();
        assert_eq!(response.exchange.ordinal, 611);
        assert_eq!(response.position, Position::Close);
        assert_eq!(response.payload, Payload::Refusal(expected.clone()));
        assert!(read_packet(&peer).is_none());
    }
    let rows = trace_through(reader, vec![], sequence, watch).unwrap();
    let sequence_text = sequence.0.to_string();
    let row = rows
        .iter()
        .find(|r| r["sequence"].as_str() == Some(sequence_text.as_str()))
        .unwrap();
    assert_eq!(row["kind"], "refusal");
    assert_eq!(
        row["payload"],
        serde_json::to_value(RefusalRecord::Lifecycle {
            asked: LifecycleAsk::PlanUpdate,
            refusal: expected
        })
        .unwrap()
    );
    assert!(row.get("turn").is_none());
    assert!(row.get("causal_parent").is_none());
    assert_eq!(owner.declined_plan.as_ref().unwrap().wire(), bytes);
    guard.input = Some(guard.management.take().unwrap().finish().ok().unwrap());
    let context = &mut guard.input.as_mut().unwrap().serving.owner.context;
    assert!(context.deadlines.get(PumpDeadline::PlanRefusal).is_none());
    assert_eq!(context.deadlines.get(PumpDeadline::Decision), earlier);
}
