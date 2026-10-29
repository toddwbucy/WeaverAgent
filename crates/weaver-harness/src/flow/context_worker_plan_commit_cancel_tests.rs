//! Stop arriving while a real SQLite filing receipt is held must drain that
//! exact filing. A committed effect also owes its actual summary refresh.
use super::*;
use crate::flow::clock::{ReadyFd, WaitInterest, WaitOutcome};

// The test advances the socket proxy between worker steps to hold an exact
// receipt boundary. Real monotonic readings remain in use; blocking and CPU
// behavior have separate real-poll process cases.
struct StepIo;
impl PumpIo for StepIo {
    fn now(&mut self) -> std::io::Result<u64> {
        MonotonicIo.now()
    }
    fn wait(
        &mut self,
        interests: &[WaitInterest<'_>],
        _: Option<u64>,
    ) -> std::io::Result<WaitOutcome> {
        // The fixture polls actual readiness, then yields to its in-thread
        // proxy. It never reports expiration of an unexpired owner deadline.
        let mut fds: Vec<_> = interests
            .iter()
            .map(|interest| nix::poll::PollFd::new(interest.fd, interest.events))
            .collect();
        nix::poll::poll(&mut fds, nix::poll::PollTimeout::ZERO)
            .map_err(|error| std::io::Error::from_raw_os_error(error as i32))?;
        Ok(WaitOutcome::Ready(
            fds.iter()
                .enumerate()
                .filter_map(|(index, fd)| {
                    let events = fd.revents().unwrap();
                    (!events.is_empty()).then_some(ReadyFd { index, events })
                })
                .collect(),
        ))
    }
}

pub(super) fn exercise(
    serving: FoundationServing<'_>,
    member: &mut Member,
    reader: &mut UnixStream,
    socket: &Path,
    after_effect: bool,
) {
    let mut guard = Guard {
        input: Some(serving.input()),
        management: None,
    };
    let document = document();
    let original = request(socket, &document);
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
    let mut holding = !after_effect;
    if holding {
        member.hold_next_normal_filing();
    }
    while !member.normal_filing_held() {
        let input = guard.input.as_mut().unwrap();
        assert_ne!(
            input.advance_with_io(&mut StepIo),
            FoundationInputProgress::Stopped,
            "{:?}",
            input.detail()
        );
        if !holding
            && input
                .serving
                .owner
                .context
                .root_plan_work()
                .unwrap()
                .input_filing()
                .is_some()
        {
            // The input receipt is now consumed. The proxy has not forwarded
            // the next member response, so the first intercepted filing is
            // the effect, even if its request has already reached the socket.
            member.hold_next_normal_filing();
            holding = true;
        }
        member.step_live("ready");
        assert!(Instant::now() < end, "normal member filing was never held");
        std::thread::sleep(Duration::from_millis(1));
    }
    let work = guard
        .input
        .as_ref()
        .unwrap()
        .serving
        .owner
        .context
        .root_plan_work()
        .unwrap();
    assert_eq!(work.input_filing().is_some(), after_effect);
    assert!(
        work.effect_filing().is_none(),
        "held receipt cannot grant completion"
    );
    let peer = dial(socket);
    let request = OrganEnvelope {
        exchange: ExchangeId {
            opener: Opener::Admin,
            ordinal: 59,
        },
        position: Position::Open,
        payload: Payload::Directive(LifecycleDirective::Stop {
            cause: Cause { uid: 0 },
        }),
    };
    let bytes = serde_json::to_vec(&request).unwrap();
    assert_eq!(
        send(peer.as_raw_fd(), &bytes, MsgFlags::MSG_NOSIGNAL).unwrap(),
        bytes.len()
    );
    while guard.input.as_mut().unwrap().advance_with_io(&mut StepIo)
        != FoundationInputProgress::ManagementPending
    {
        assert!(Instant::now() < end);
    }
    guard.management = Some(guard.input.take().unwrap().management().ok().unwrap());
    loop {
        let owner = guard.management.as_mut().unwrap();
        assert_eq!(
            owner.advance_with_io(&mut StepIo),
            FoundationManagementProgress::Pending,
            "Stop cannot finish before the committed receipt returns"
        );
        let context = &owner.input.serving.owner.context;
        assert_eq!(context.deadlines.get(PumpDeadline::Decision), Some(bound));
        if context.root_plan_work().unwrap().is_cancelled() {
            break;
        }
        assert!(Instant::now() < end);
    }
    assert!(read_packet(&peer).is_none());
    assert!(read_packet(&original).is_none());
    member.release_normal_filing();
    loop {
        let owner = guard.management.as_mut().unwrap();
        let progress = owner.advance_with_io(&mut StepIo);
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
        assert!(
            Instant::now() < end,
            "Stop did not drain the admitted filing and refresh"
        );
        std::thread::sleep(Duration::from_millis(1));
    }
    let answer: OrganEnvelope = serde_json::from_slice(&read_packet(&peer).unwrap()).unwrap();
    assert_eq!(answer.exchange, request.exchange);
    assert_eq!(
        answer.payload,
        Payload::Answer(weaver_types::LifecycleAnswer::AtRest)
    );
    guard.input = Some(guard.management.take().unwrap().finish().ok().unwrap());
    let input = guard.input.as_mut().unwrap();
    assert!(!input.plan_pending());
    let context = &mut input.serving.owner.context;
    assert!(context.failure().is_none());
    assert!(context.root_plan_work().is_none());
    assert!(context.read.is_none() && context.filing.is_none() && context.plan_summary.is_none());
    assert!(context.deadlines.get(PumpDeadline::Decision).is_none());
    let Some(RootPlanCompletion::Cancelled(account)) =
        &input.serving.plan.as_ref().unwrap().completion
    else {
        panic!("Stop must retain cancelled custody")
    };
    assert_eq!(account.packet().input().2, document);
    assert!(
        account.work().input_filing().is_some(),
        "all input dispositions must finish"
    );
    assert_eq!(account.work().effect_filing().is_some(), after_effect);
    assert_eq!(account.work().summary().is_some(), after_effect);
    let summary = context.committed_plan_summary().unwrap().cloned();
    assert_eq!(summary.is_some(), after_effect);
    if let Some(summary) = &summary {
        assert_eq!(
            summary.document_sha256.as_str(),
            format!("{:x}", Sha256::digest(document.as_bytes()))
        );
        assert_eq!(summary.status, s::PlanStatus::Locked);
        assert_eq!(
            summary.last_source.block_id.sequence.0,
            account.work().effect_filing().unwrap().sequence().0
        );
    }
    assert!(
        read_packet(&original).is_none(),
        "cancelled root cannot answer PlanUpdated"
    );
    let s::enter::ObservationContent::Answer(s::enter::ObservationAnswer::State { plan, .. }) =
        observe(&mut guard, socket)
    else {
        panic!("settled operation must be observable")
    };
    assert_eq!(plan, summary);
    let rows = trace(reader);
    assert_eq!(
        rows.iter()
            .filter(|row| row["kind"] == "plan.event")
            .count(),
        if after_effect { 2 } else { 1 }
    );
    assert!(
        !rows
            .iter()
            .any(|row| row["kind"] == "fault" || row["kind"] == "turn.started")
    );
}
