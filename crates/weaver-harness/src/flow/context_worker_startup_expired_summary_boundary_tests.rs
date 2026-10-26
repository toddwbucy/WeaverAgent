//! Actual summary read settlement with expiry at each final clock.
use super::*;
use crate::flow::context::plan_summary_driver::PlanSummaryDisposition;
use crate::flow::context::{PlanSummaryProgress, RecordedRead, RecordingContext};
use crate::flow::plan::PlanSummaryUnavailable;
use crate::flow::read::ReadFailure;
use weaver_trace::Sequence;
use weaver_trace::serving::Kind;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Boundary {
    Recorded,
    ReadComplete,
    ReadSettled,
    Refused,
}

impl Boundary {
    fn from_mode(mode: &str) -> Option<Self> {
        match mode {
            "expired-startup-summary-boundary-recorded-populated-complete" => Some(Self::Recorded),
            "expired-startup-summary-boundary-read-complete-populated-complete" => {
                Some(Self::ReadComplete)
            }
            "expired-startup-summary-boundary-read-settled-populated-complete" => {
                Some(Self::ReadSettled)
            }
            "expired-startup-summary-boundary-refused-complete" => Some(Self::Refused),
            _ => None,
        }
    }

    fn sample(self) -> usize {
        // Begin with a released result, no pending author and an unselected
        // recall. The first four samples are summary live, read live, recall
        // live and author preparation. These final three samples are the
        // post-Recorded check, the read's final live, and the owner's final live.
        // Assertions below pin the reached state, so adding a clock cannot
        // silently turn one named boundary into another.
        match self {
            Self::Recorded => 5,
            Self::ReadComplete => 6,
            Self::ReadSettled => 7,
            // The refusal already completed its read. With the coordinator
            // idle, samples are summary/read/reducer live, coordinator time,
            // reducer final live, then the summary owner's final live.
            Self::Refused => 6,
        }
    }
}

pub(super) fn is_mode(mode: &str) -> bool {
    Boundary::from_mode(mode).is_some()
}

pub(super) fn is_settled(mode: &str) -> bool {
    matches!(
        Boundary::from_mode(mode),
        Some(Boundary::ReadSettled | Boundary::Refused)
    )
}

fn refusal() -> ReadFailure {
    ReadFailure::Refused {
        code: s::ReadRefusalCode::ResultTooLarge,
        detail: "original boot summary refusal".into(),
    }
}

fn ports<I: PumpIo>(io: &mut I) -> PumpPorts<'_, I> {
    PumpPorts {
        lifecycle: LifecyclePorts {
            listener: None,
            peer: None,
            residency: None,
            dial: None,
            leave_peer: None,
        },
        gate: None,
        decode: None,
        label: None,
        io,
    }
}

pub(super) fn prepare(
    context: &mut RecordingContext,
    member: &mut Member,
    clock: &mut FakePumpIo,
    limit: Instant,
    mode: &str,
) {
    let refused = Boundary::from_mode(mode) == Some(Boundary::Refused);
    let enter = context.deadlines.get(PumpDeadline::Enter).unwrap();
    let requests = member.requests.len();
    let measurements = context
        .barrier
        .recorder()
        .structure()
        .by_kind(Kind::StateStoreMeasurement)
        .count();
    // The original Enter must cap this read. An independently earlier read
    // timeout is a component failure and is a different fixture.
    clock.advance_to(enter.deadline_ns() - 1_000_000).unwrap();
    member.hold_control = Some(if refused { "summary" } else { "release" });
    while member.held_control.is_none() {
        assert!(Instant::now() < limit, "summary reply did not arrive");
        let progress = context
            .advance_plan_summary(&mut ports(&mut CircuitIo(clock)))
            .unwrap();
        assert_ne!(progress, PlanSummaryProgress::Complete);
        member.step(
            clock.now().unwrap(),
            if refused { "summary-refused" } else { "ready" },
        );
        if progress == PlanSummaryProgress::Pending {
            context
                .pump_once(&mut ports(&mut CircuitIo(clock)))
                .unwrap();
        }
    }
    if refused {
        assert!(matches!(
            member.held_control.as_ref().unwrap(),
            s::StateFrame::ControlAnswer {
                frame: s::StateControlAnswer::ControlRefused {
                    code: s::ControlRefusalCode::Read(s::ReadRefusalCode::ResultTooLarge),
                    detail,
                },
                ..
            } if detail == "original boot summary refusal"
        ));
    } else {
        assert!(matches!(
            member.held_control.as_ref().unwrap(),
            s::StateFrame::ControlAnswer {
                frame: s::StateControlAnswer::Released { .. },
                ..
            }
        ));
    }
    member.hold_control = None;
    member.step(clock.now().unwrap(), "ready");

    // Reduce the real reply and its actual Recorded operation measurement,
    // stopping before summary completion (and before recall for a success).
    // This is the ordinary read settlement seam, with no constructed proof.
    while if refused {
        context.read.as_ref().unwrap().plan_refusal().is_none()
    } else {
        !context.read_blocks_reduction()
    } {
        assert!(Instant::now() < limit, "summary reply did not settle");
        context
            .pump_once(&mut ports(&mut CircuitIo(clock)))
            .unwrap();
        context.reduce_flow(&mut CircuitIo(clock)).unwrap();
    }
    if refused {
        // Retain the real refused read while finishing its coordinator notices.
        // No summary completion or replacement read is invoked by this drain.
        while context.reduce_flow(&mut CircuitIo(clock)).unwrap()
            != crate::flow::coordinator::Advance::Idle
        {
            assert!(
                Instant::now() < limit,
                "refused summary flow did not settle"
            );
        }
        assert_eq!(
            context
                .barrier
                .recorder()
                .structure()
                .by_kind(Kind::StateStoreMeasurement)
                .count(),
            measurements + 1,
            "the original refusal has its actual Recorded measurement"
        );
    }
    let read = context.read.as_ref().unwrap();
    assert_eq!(read.plan_bound().deadline_ns(), enter.deadline_ns());
    assert!(read.plan_expected().is_none());
    assert_eq!(read.plan_recall().is_some(), !refused);
    assert_eq!(read.plan_refusal(), refused.then_some(&refusal()));
    assert!(read.plan_recorded().is_none());
    assert!(read.plan_ticket().is_none());
    assert!(context.barrier.pending().is_none());
    assert!(context.barrier.pending_author().is_none());
    assert!(!context.flow.has_pending_record());
    assert!(context.flow.operation_completion().is_none());
    assert!(context.previous_plan_summary().is_none());
    // Make the next single recall admission independent of writer scheduling.
    // This only observes the actual writer; it neither constructs Recorded nor
    // changes the retained clock. Pressure has separate composition fixtures.
    while context.barrier.drain_status(&mut context.budget).unwrap()
        != weaver_trace::serving::DrainStatus::Emptied
    {
        assert!(
            Instant::now() < limit,
            "summary trace predecessor did not drain"
        );
        std::thread::sleep(Duration::from_millis(1));
    }
    let expected: &[&str] = if refused {
        &["summary"]
    } else {
        &["summary", "release"]
    };
    assert_eq!(&member.requests[requests..], expected);
}

struct BoundaryClock<'a> {
    clock: &'a mut FakePumpIo,
    deadline: u64,
    boundary: usize,
    samples: usize,
}

impl PumpIo for BoundaryClock<'_> {
    fn now(&mut self) -> std::io::Result<u64> {
        self.samples += 1;
        if self.samples == self.boundary {
            self.clock.advance_to(self.deadline)?;
        }
        self.clock.now()
    }

    fn wait(
        &mut self,
        _: &[WaitInterest<'_>],
        _: Option<u64>,
    ) -> std::io::Result<crate::flow::clock::WaitOutcome> {
        panic!("settled summary needs no transport wait");
    }
}

pub(super) fn expire(context: &mut RecordingContext, clock: &mut FakePumpIo, mode: &str) {
    let boundary = Boundary::from_mode(mode).unwrap();
    let refused = boundary == Boundary::Refused;
    let enter = context.deadlines.get(PumpDeadline::Enter).unwrap();
    let authored_ns = clock.now().unwrap();
    let before = context
        .barrier
        .recorder()
        .structure()
        .by_kind(Kind::Recall)
        .count();
    let mut io = BoundaryClock {
        clock,
        deadline: enter.deadline_ns(),
        boundary: boundary.sample(),
        samples: 0,
    };
    assert_eq!(
        context.advance_plan_summary(&mut ports(&mut io)),
        Err(RunFailure::Wait {
            key: PumpDeadline::Enter,
            bound: enter,
        })
    );
    assert!(io.samples >= boundary.sample());
    assert_eq!(context.last_ns, enter.deadline_ns());
    assert_eq!(
        context
            .barrier
            .recorder()
            .structure()
            .by_kind(Kind::Recall)
            .count(),
        before + usize::from(!refused),
        "only a successful read has an actual Recorded recall"
    );
    if !refused {
        let recall: serde_json::Value = serde_json::from_str(
            &context
                .barrier
                .recorder()
                .structure()
                .by_kind(Kind::Recall)
                .last()
                .unwrap()
                .line,
        )
        .unwrap();
        assert_eq!(
            recall["monotonic_ns"],
            (authored_ns - context.origin_ns()).to_string(),
            "the recall author must precede the Enter deadline"
        );
    }
    let work = context.plan_summary.as_ref().unwrap();
    assert_eq!(
        work.disposition_for_test(),
        PlanSummaryDisposition {
            complete: false,
            cancelled: false,
            key: PumpDeadline::Enter,
            bound: enter,
            account_deadline: enter.deadline_ns(),
        }
    );
    assert_eq!(work.expired_test_settlement(), (is_settled(mode), false));
    if refused {
        assert!(work.expired_test_recorded_read().is_none());
        assert_eq!(work.expired_test_refusal(), Some(&refusal()));
    } else {
        let proof = work.expired_test_recorded_read().unwrap();
        assert!(proof.belongs_to(context));
        assert_eq!(
            context.read.as_ref().and_then(|read| read.plan_recorded()),
            (!is_settled(mode)).then_some(proof.recall_sequence())
        );
        assert!(work.expired_test_refusal().is_none());
    }
    assert_eq!(context.read.is_none(), is_settled(mode));
    assert_eq!(
        context.deadlines.get(PumpDeadline::StateControl).is_some(),
        boundary == Boundary::Recorded
    );
    assert_eq!(
        context.read_blocks_reduction(),
        boundary == Boundary::Recorded
    );
    assert!(context.previous_plan_summary().is_none());
    assert_eq!(
        context.committed_plan_summary(),
        Err(PlanSummaryUnavailable::Pending)
    );
    assert!(context.failure().is_none());
    assert!(context.flow.halt().is_none());
    assert!(context.barrier.pending().is_none());
    assert!(context.barrier.pending_author().is_none());
    assert!(context.completed_author.is_none());
}

pub(super) struct Evidence {
    outcome: Outcome,
    disposition: PlanSummaryDisposition,
}

enum Outcome {
    Recorded {
        raw: Vec<u8>,
        sequence: Sequence,
        address: *const RecordedRead,
    },
    Refused {
        refusal: ReadFailure,
        measurement: serde_json::Value,
    },
}

pub(super) fn evidence(
    context: &RecordingContext,
    member: &Member,
    mode: &str,
) -> Option<Evidence> {
    let boundary = Boundary::from_mode(mode)?;
    let work = context.plan_summary.as_ref().unwrap();
    let outcome = if boundary == Boundary::Refused {
        assert!(work.expired_test_recorded_read().is_none());
        let measurement: serde_json::Value = serde_json::from_str(
            &context
                .barrier
                .recorder()
                .structure()
                .by_kind(Kind::StateStoreMeasurement)
                .last()
                .unwrap()
                .line,
        )
        .unwrap();
        assert_eq!(measurement["payload"]["observation"]["operation"], "read");
        Outcome::Refused {
            refusal: work.expired_test_refusal().unwrap().clone(),
            measurement,
        }
    } else {
        let read = work.expired_test_recorded_read().unwrap();
        let raw = serde_json::to_vec(member.summary.as_ref().expect("populated summary fixture"))
            .unwrap();
        assert_eq!(read.raw_bytes(), raw);
        Outcome::Recorded {
            raw,
            sequence: read.recall_sequence(),
            address: std::ptr::from_ref(read),
        }
    };
    Some(Evidence {
        outcome,
        disposition: work.disposition_for_test(),
    })
}

pub(super) fn assert_cleanup(owner: &FoundationExpiredRollback, expected: &Evidence, mode: &str) {
    let active = owner.active.as_ref().unwrap();
    let account = if is_settled(mode) {
        assert!(active.read.is_none());
        assert!(
            active.current.is_some(),
            "Leave licenses the cleanup Grants read"
        );
        active.summary.as_ref().unwrap()
    } else {
        assert!(active.summary.is_none());
        assert!(
            active.current.is_none(),
            "the stopped read admits no new Grants"
        );
        let Some(crate::flow::context::expired_read_driver::ExpiredReadAccount::Summary(account)) =
            active.read.as_ref()
        else {
            panic!("retain the actual summary read account");
        };
        account
    };
    match &expected.outcome {
        Outcome::Recorded {
            raw,
            sequence,
            address,
        } => {
            let read = account.recorded_read().unwrap();
            assert_eq!(read.raw_bytes(), raw);
            assert_eq!(read.recall_sequence(), *sequence);
            assert_eq!(std::ptr::from_ref(read), *address);
            assert!(account.refusal().is_none());
        }
        Outcome::Refused { refusal, .. } => {
            assert!(account.recorded_read().is_none());
            assert_eq!(account.refusal(), Some(refusal));
        }
    }
    let mut disposition = expected.disposition;
    disposition.cancelled = !is_settled(mode);
    assert_eq!(account.disposition_for_test(), disposition);
    assert_eq!(account.incomplete().is_some(), !is_settled(mode));
    assert!(account.repaired().is_some());
    assert!(owner.run.context.previous_plan_summary().is_none());
}

pub(super) fn assert_trace(rows: &[serde_json::Value], expected: &Evidence) {
    let recalls: Vec<_> = rows
        .iter()
        .filter(|row| {
            row["kind"] == "recall"
                && row["payload"]["read"]["selector"]
                    .get("plan_current")
                    .is_some()
        })
        .collect();
    match &expected.outcome {
        Outcome::Recorded { sequence, .. } => {
            assert_eq!(
                recalls.len(),
                1,
                "rollback must not duplicate the summary recall"
            );
            assert_eq!(recalls[0]["sequence"], sequence.0.to_string());
        }
        Outcome::Refused { measurement, .. } => {
            assert!(
                recalls.is_empty(),
                "a typed refusal grants no summary recall"
            );
            let measured: Vec<_> = rows
                .iter()
                .filter(|row| row["sequence"] == measurement["sequence"])
                .collect();
            assert_eq!(measured, [measurement]);
        }
    }
}
