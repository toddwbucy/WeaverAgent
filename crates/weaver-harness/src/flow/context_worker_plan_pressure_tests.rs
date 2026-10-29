//! Real socket pressure after settlement cannot renew the operation deadline,
//! discard its recorded result or turn a missing answer into a foundation halt.
use super::*;
use crate::flow::clock::{WaitInterest, WaitOutcome};
use nix::sys::socket::recv;

struct ObserveWait {
    deadline: u64,
    waits: usize,
}
impl PumpIo for ObserveWait {
    fn now(&mut self) -> std::io::Result<u64> {
        MonotonicIo.now()
    }
    fn wait(
        &mut self,
        interests: &[WaitInterest<'_>],
        deadline: Option<u64>,
    ) -> std::io::Result<WaitOutcome> {
        assert_eq!(
            deadline,
            Some(self.deadline),
            "answer must use the accepted Decision deadline"
        );
        self.waits += 1;
        MonotonicIo.wait(interests, deadline)
    }
}

fn fill(peer: &LifecyclePeer) -> usize {
    let bytes = [b'x'; 65536];
    let mut packets = 0;
    // A rejected large packet can leave space for a small answer. Fill that
    // remainder with one-byte packets as well before claiming backpressure.
    for bytes in [&bytes[..], &bytes[..1]] {
        loop {
            match send(
                peer.fd().as_raw_fd(),
                bytes,
                MsgFlags::MSG_DONTWAIT | MsgFlags::MSG_NOSIGNAL,
            ) {
                Ok(n) => {
                    assert_eq!(n, bytes.len());
                    packets += 1;
                }
                Err(nix::errno::Errno::EAGAIN) => break,
                other => panic!("fill root answer socket: {other:?}"),
            }
        }
    }
    assert!(packets > 0);
    packets
}

fn drain_fill(peer: &OwnedFd, packets: usize) {
    let mut bytes = [0; 65536];
    for _ in 0..packets {
        let n = recv(peer.as_raw_fd(), &mut bytes, MsgFlags::MSG_DONTWAIT).unwrap();
        assert!(n > 0 && bytes[..n].iter().all(|b| *b == b'x'));
    }
}

pub(super) fn exercise(
    serving: FoundationServing<'_>,
    reader: &mut UnixStream,
    socket: &Path,
    expires: bool,
) {
    let mut guard = Guard {
        input: Some(serving.input()),
        management: None,
    };
    let document = "{ root malformed answer pressure";
    let peer = request(socket, document);
    accepted(&mut guard);
    let bound = guard
        .input
        .as_ref()
        .unwrap()
        .serving
        .plan
        .as_ref()
        .unwrap()
        .bound;
    let end = Instant::now() + Duration::from_secs(10);
    while guard
        .input
        .as_ref()
        .unwrap()
        .serving
        .plan
        .as_ref()
        .unwrap()
        .completion
        .is_none()
    {
        let input = guard.input.as_mut().unwrap();
        assert_ne!(
            input.advance(),
            FoundationInputProgress::Stopped,
            "{:?}",
            input.detail()
        );
        assert!(Instant::now() < end);
    }
    let input = guard.input.as_mut().unwrap();
    assert!(input.serving.owner.context.root_plan_work().is_none());
    assert!(
        input
            .serving
            .owner
            .context
            .deadlines
            .get(PumpDeadline::Decision)
            .is_none()
    );
    let plan = input.serving.plan.as_ref().unwrap();
    assert!(plan.send.is_none());
    let Some(RootPlanCompletion::Refused { account, .. }) = &plan.completion else {
        panic!()
    };
    let sequence = account.work().refusal_sequence().unwrap();
    let packets = fill(plan.peer.as_ref().unwrap());
    let cpu = thread_cpu_ns();
    let mut clock = ObserveWait {
        deadline: bound.deadline_ns(),
        waits: 0,
    };
    std::thread::scope(|scope| {
        let drain = (!expires).then(|| {
            scope.spawn(|| {
                std::thread::sleep(Duration::from_millis(200));
                drain_fill(&peer, packets);
            })
        });
        while input.plan_pending() {
            assert_ne!(
                input.advance_with_io(&mut clock),
                FoundationInputProgress::Stopped,
                "{:?}",
                input.detail()
            );
            assert!(
                thread_cpu_ns() - cpu < 120_000_000,
                "root answer pressure spun instead of waiting"
            );
            assert!(Instant::now() < end);
        }
        if let Some(drain) = drain {
            drain.join().unwrap();
        }
    });
    assert!(
        clock.waits > 0,
        "full socket must reach the real blocking wait"
    );
    let context = &input.serving.owner.context;
    assert!(context.failure().is_none());
    assert!(context.deadlines.get(PumpDeadline::PlanAnswer).is_none());
    let plan = input.serving.plan.as_ref().unwrap();
    assert!(plan.peer.is_none() && plan.retired.is_some());
    let Some(RootPlanCompletion::Refused { refusal, account }) = &plan.completion else {
        panic!()
    };
    assert_eq!(account.packet().input().2, document);
    assert_eq!(account.work().refusal_sequence(), Some(sequence));
    if expires {
        assert!(!plan.send.as_ref().unwrap().sent());
        assert!(MonotonicIo.now().unwrap() >= bound.deadline_ns());
        drain_fill(&peer, packets);
        assert!(read_packet(&peer).is_none());
    } else {
        assert!(plan.send.as_ref().unwrap().sent());
        let answer: OrganEnvelope = serde_json::from_slice(&read_packet(&peer).unwrap()).unwrap();
        assert_eq!(answer.exchange, account.packet().envelope().exchange);
        assert_eq!(answer.position, Position::Close);
        assert_eq!(answer.payload, Payload::Refusal(refusal.clone()));
        assert!(read_packet(&peer).is_none());
    }
    let rows = trace(reader);
    let sequence_text = sequence.0.to_string();
    assert_eq!(
        rows.iter()
            .filter(|row| row["kind"] == "refusal"
                && row["sequence"].as_str() == Some(sequence_text.as_str()))
            .count(),
        1
    );
    assert!(
        !rows
            .iter()
            .any(|row| row["kind"] == "fault" || row["kind"] == "plan.event")
    );
}
