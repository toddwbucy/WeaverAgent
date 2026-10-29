use super::super::failed::tests::{context, reached};
use super::super::tests::{EnterGuard, Member, dial, packet, reap};
use super::*;
use crate::flow::clock::testing::FakePumpIo;
use crate::flow::context::startup_composition::tests::CircuitIo;
use crate::flow::context::{BootPairsProgress, InspectionKind};
use crate::flow::coordinator::{FlowRecording, OperationCompletion};
use crate::flow::pump::PumpPorts;
use nix::sys::socket::{MsgFlags, send};
use std::io::Read;
use std::os::fd::{AsRawFd, OwnedFd};
use std::os::unix::net::UnixStream;
use std::path::Path;
use std::time::{Duration, Instant};
use weaver_types::state_flow as s;

#[path = "context_worker_startup_expired_summary_boundary_tests.rs"]
mod summary_boundary;

#[path = "context_worker_startup_expired_gate_tests.rs"]
mod gate_cases;

#[path = "context_worker_startup_expired_gate_failure_tests.rs"]
mod gate_failure;

#[path = "context_worker_startup_expired_gate_last_word_tests.rs"]
mod gate_last_word;

struct Guard(Option<Box<FoundationExpiredRollback>>);
fn boot_pair_account(
    owner: &FoundationExpiredRollback,
) -> Option<&crate::flow::context::boot_pair_driver::StoppedBootPairsAccount> {
    let active = owner.active.as_ref()?;
    match active.read.as_ref() {
        Some(crate::flow::context::expired_read_driver::ExpiredReadAccount::BootPairs(account)) => {
            Some(account)
        }
        _ => active.boot_pairs.as_ref(),
    }
}
impl Drop for Guard {
    fn drop(&mut self) {
        let Some(owner) = self.0.as_mut() else {
            return;
        };
        if let Some(child) = owner
            .handoff
            .gate
            .as_mut()
            .and_then(|arms| arms.child.as_mut())
        {
            reap(child);
        }
        owner.run.resources.reap_startup_test_children();
        owner.run.context.barrier.abandon();
    }
}
struct ExitGuard(Box<super::failure::FoundationExpiredRollbackExit>);
impl Drop for ExitGuard {
    fn drop(&mut self) {
        self.0.reap_composition_test_children();
    }
}
struct PanicReader(UnixStream);
impl Drop for PanicReader {
    fn drop(&mut self) {
        if std::thread::panicking() {
            let _ = self.0.shutdown(std::net::Shutdown::Both);
        }
    }
}

pub(in crate::flow::context::worker_startup) fn refusal(
    enter: FoundationEnter<'_>,
    member: &mut Member,
    clock: &mut FakePumpIo,
) {
    let mut guard = EnterGuard(Some(enter));
    let limit = Instant::now() + Duration::from_secs(8);
    loop {
        assert!(Instant::now() < limit, "classifier refusal did not arrive");
        // SAFETY: the fixture retains its thread and reaps every actual child.
        if unsafe {
            guard
                .0
                .as_mut()
                .unwrap()
                .advance_with_io(&mut CircuitIo(clock))
        } == FoundationEnterProgress::Stopped
        {
            break;
        }
        member.step(clock.now().unwrap(), "ready");
        std::thread::sleep(Duration::from_millis(1));
    }
    let context = context(guard.0.as_mut().unwrap());
    assert!(context.failure().is_none());
    assert!(
        !context
            .deadlines
            .get(PumpDeadline::Enter)
            .unwrap()
            .expired(context.last_ns)
    );
    let expected = guard.0.as_ref().unwrap().detail().unwrap().to_owned();
    assert!(expected.contains("ClassifierRefused"));
    guard.0 = Some(
        guard
            .0
            .take()
            .unwrap()
            .expired_handoff()
            .err()
            .expect("an actual timely refusal is not expired Enter authority"),
    );
    assert_eq!(guard.0.as_ref().unwrap().detail(), Some(expected.as_str()));
}

pub(in crate::flow::context::worker_startup) fn enter(
    enter: FoundationEnter<'_>,
    member: &mut Member,
    reader: &mut UnixStream,
    clock: &mut FakePumpIo,
    mode: &str,
    root: &Path,
) -> Box<FoundationExpiredEnter> {
    let _reader = PanicReader(reader.try_clone().unwrap());
    let mut guard = EnterGuard(Some(enter));
    let limit = Instant::now() + Duration::from_secs(8);
    let suffix = mode.strip_prefix("expired-startup-").unwrap();
    let suffix = suffix.split("-release-").next().unwrap();
    let suffix = suffix.strip_suffix("-cap").unwrap_or(suffix);
    let suffix = suffix.strip_suffix("-trace").unwrap_or(suffix);
    let suffix = suffix.strip_suffix("-complete").unwrap_or(suffix);
    let suffix = suffix.strip_suffix("-record").unwrap_or(suffix);
    let suffix = suffix.strip_suffix("-pending").unwrap_or(suffix);
    let suffix = suffix.strip_suffix("-varied").unwrap_or(suffix);
    let suffix = suffix.strip_suffix("-unreadable").unwrap_or(suffix);
    let suffix = suffix.strip_suffix("-held").unwrap_or(suffix);
    let suffix = suffix.strip_suffix("-cursor").unwrap_or(suffix);
    let suffix = suffix.strip_suffix("-measurement").unwrap_or(suffix);
    let template_pressure = mode.contains("-template-record-")
        || mode.contains("-template-component-")
        || mode.contains("-template-foreign-");
    let summary_account =
        mode.starts_with("expired-startup-summary-account-") || summary_boundary::is_mode(mode);
    let target = if gate_cases::is_mode(mode) {
        "failed-startup-raised".to_owned()
    } else if summary_account {
        "failed-startup-summary-read".to_owned()
    } else if mode.starts_with("expired-startup-repair-") {
        "failed-startup-repair".to_owned()
    } else if mode.starts_with("expired-startup-admission-") {
        "failed-startup-before-classifier".to_owned()
    } else if template_pressure {
        "failed-startup-template-record".to_owned()
    } else if mode.contains("-template-completion-") {
        "failed-startup-template-recorded".to_owned()
    } else {
        format!("failed-startup-{suffix}")
    };
    let mut template_pressure_filled = false;
    let mut template_probe_interrupted = false;
    let admission_received = mode
        .starts_with("expired-startup-classifying")
        .then(|| root.join("classifier-bootstrap"));
    let repair_filing = mode.starts_with("expired-startup-repair-filing-");
    if summary_account && mode.contains("-populated-") {
        let corpus: Vec<serde_json::Value> =
            serde_json::from_str(include_str!("fixtures/recall-sources.json")).unwrap();
        member.summary = Some(
            serde_json::from_value(
                corpus
                    .iter()
                    .find(|row| row["case"] == "plan_current_populated")
                    .unwrap()["result"]
                    .clone(),
            )
            .unwrap(),
        );
    }

    if matches!(
        mode,
        "expired-startup-repair-filed-complete" | "expired-startup-repair-proposed-complete"
    ) || repair_filing
    {
        let corpus: Vec<serde_json::Value> =
            serde_json::from_str(include_str!("fixtures/recall-sources.json")).unwrap();
        let mut pair = corpus
            .iter()
            .find(|case| case["case"] == "open_pairs_original_blocks")
            .unwrap()["result"]["open_pairs"]["pairs"][0]
            .clone();
        fn prior_run(value: &mut serde_json::Value) {
            match value {
                serde_json::Value::String(s) if s == "run" => *s = "expired-previous-run".into(),
                serde_json::Value::Array(xs) => xs.iter_mut().for_each(prior_run),
                serde_json::Value::Object(xs) => xs.values_mut().for_each(prior_run),
                _ => {}
            }
        }
        prior_run(&mut pair);
        member
            .boot_pairs
            .push(serde_json::from_value(pair).unwrap());
    }
    if repair_filing
        && (mode.contains("-pending-") || mode.contains("-measurement-") || mode.contains("-flow-"))
    {
        member.hold_kind = Some("tool.call.completed");
    }
    let read_target = matches!(suffix, "prompt" | "summary-read")
        || mode.starts_with("expired-startup-repair-read-")
        || mode.starts_with("expired-startup-repair-recheck-");
    if read_target && !mode.contains("-cursor-") {
        member.hold_control = Some(if mode.starts_with("expired-startup-repair-read-") {
            "pairs"
        } else if mode.starts_with("expired-startup-repair-recheck-") {
            "pairs-recheck"
        } else if suffix == "prompt" {
            "prompt"
        } else {
            "summary"
        });
    }
    while !(if mode == "expired-startup-installed-unsent-complete" {
        matches!(guard.0.as_ref().unwrap().state.as_ref(), Some(EnterState::Gate(gate))
            if gate.stage() == crate::flow::context::boot_composition::gate_startup::GateStartupStage::Raise)
    } else if gate_cases::is_mode(mode) {
        gate_cases::reached(guard.0.as_ref().unwrap(), mode)
    } else {
        reached(guard.0.as_ref().unwrap(), &target)
    }) || (read_target && member.held_control.is_none())
        || admission_received
            .as_ref()
            .is_some_and(|path| !path.exists())
    {
        assert!(
            Instant::now() < limit,
            "target {target}: {:?}",
            guard.0.as_ref().unwrap().detail()
        );
        if template_pressure
            && !template_pressure_filled
            && reached(guard.0.as_ref().unwrap(), "failed-startup-boot")
        {
            super::super::failed::tests::fill_trace(context(guard.0.as_mut().unwrap()), clock);
            template_pressure_filled = true;
        }
        if (mode.contains("-template-recorded-") || mode.contains("-template-completion-"))
            && !template_probe_interrupted
            && reached(guard.0.as_ref().unwrap(), "failed-startup-boot")
        {
            context(guard.0.as_mut().unwrap())
                .flow
                .interrupt_classifier_terminal_probes_for_test(8);
            template_probe_interrupted = true;
        }
        // SAFETY: the original fixture guards reap all actual children.
        let progress = unsafe {
            guard
                .0
                .as_mut()
                .unwrap()
                .advance_with_io(&mut CircuitIo(clock))
        };
        assert_eq!(
            progress,
            FoundationEnterProgress::Pending,
            "{:?}",
            guard.0.as_ref().unwrap().detail()
        );
        if read_target && mode.contains("-cursor-") && reached(guard.0.as_ref().unwrap(), &target) {
            member.hold_control = Some("release");
        }
        member.step(clock.now().unwrap(), "ready");
        std::thread::sleep(Duration::from_millis(1));
    }
    if summary_account {
        let EnterState::Boot(owner) = guard.0.as_mut().unwrap().state.as_mut().unwrap() else {
            panic!("original summary boot owner");
        };
        owner.failure_test_stage_summary(&mut CircuitIo(clock));
        let context = context(guard.0.as_mut().unwrap());
        if summary_boundary::is_mode(mode) {
            summary_boundary::prepare(context, member, clock, limit, mode);
        } else if !mode.contains("-unstarted-") {
            loop {
                assert!(Instant::now() < limit, "original summary did not settle");
                let mut ports = PumpPorts {
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
                    io: &mut CircuitIo(clock),
                };
                let progress = context.advance_plan_summary(&mut ports).unwrap();
                if progress == crate::flow::context::PlanSummaryProgress::Complete {
                    break;
                }
                member.step(
                    clock.now().unwrap(),
                    if mode.contains("-refused-") {
                        "summary-refused"
                    } else {
                        "ready"
                    },
                );
                if progress == crate::flow::context::PlanSummaryProgress::Pending {
                    context
                        .pump_once(&mut PumpPorts {
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
                            io: &mut CircuitIo(clock),
                        })
                        .unwrap();
                }
            }
            assert_eq!(
                context.previous_plan_summary().is_some(),
                mode.contains("-populated-")
            );
        }
        assert!(context.plan_summary.is_some());
        assert_eq!(context.read.is_none(), !summary_boundary::is_mode(mode));
    }
    if [
        "expired-startup-repair-unstarted-complete",
        "expired-startup-repair-retained-complete",
        "expired-startup-repair-filed-complete",
        "expired-startup-repair-proposed-complete",
    ]
    .contains(&mode)
        || repair_filing
    {
        let proposed = mode == "expired-startup-repair-proposed-complete";
        if proposed || repair_filing {
            let context = context(guard.0.as_mut().unwrap());
            let enter = context.deadlines.get(PumpDeadline::Enter).unwrap();
            let remaining = (context
                .load_duty
                .as_ref()
                .unwrap()
                .instruction
                .decision_wait
                .elapsed_ns
                .0
                / 2)
            .min(1_000_000);
            assert!(remaining > 100);
            // Exercise a receipt wait capped by Enter. An independently
            // earlier Decision expiry still belongs to ordinary failed exit.
            clock.advance_to(enter.deadline_ns() - remaining).unwrap();
        }
        let EnterState::Boot(owner) = guard.0.as_mut().unwrap().state.as_mut().unwrap() else {
            panic!("original boot owner");
        };
        owner.failure_test_stage_repair(&mut CircuitIo(clock));
        if mode != "expired-startup-repair-unstarted-complete" {
            let context = context(guard.0.as_mut().unwrap());
            let mut filing_pressure = false;
            loop {
                assert!(Instant::now() < limit, "boot repair did not settle");
                if repair_filing
                    && mode.contains("-flow-")
                    && !filing_pressure
                    && context
                        .flow
                        .state()
                        .outstanding()
                        .first()
                        .is_some_and(|work| {
                            work.operation == s::StateOperation::FilePair && work.bytes_sent > 0
                        })
                {
                    super::super::failed::tests::fill_trace(context, clock);
                    filing_pressure = true;
                }
                let mut ports = PumpPorts {
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
                    io: &mut CircuitIo(clock),
                };
                let progress =
                    if repair_filing && mode.contains("-settled-") && context.filing.is_some() {
                        let progress = context
                            .advance_owned_filing(
                                crate::flow::context::receipt_driver::FilingOwner::BootPairs,
                                &mut ports,
                            )
                            .unwrap();
                        if progress == crate::flow::context::FilingProgress::Complete {
                            assert!(context.flow.filing_receipts().unwrap().is_complete());
                            assert!(
                                context.flow.filing_recorded_ns().unwrap()
                                    < context
                                        .deadlines
                                        .get(PumpDeadline::Enter)
                                        .unwrap()
                                        .deadline_ns()
                            );
                            break;
                        }
                        if progress == crate::flow::context::FilingProgress::Pending {
                            BootPairsProgress::Pending
                        } else {
                            BootPairsProgress::Progress
                        }
                    } else {
                        context.advance_boot_pairs(&mut ports).unwrap()
                    };
                if proposed && let Some(bound) = context.deadlines.get(PumpDeadline::Decision) {
                    assert_eq!(
                        bound.deadline_ns(),
                        context
                            .deadlines
                            .get(PumpDeadline::Enter)
                            .unwrap()
                            .deadline_ns()
                    );
                    assert!(context.filing.is_none());
                    assert!(
                        member
                            .filings
                            .iter()
                            .all(|file| !matches!(file, s::StateFrame::FilePair { .. }))
                    );
                    break;
                }
                if repair_filing
                    && (mode.contains("-unstarted-") || mode.contains("-author-"))
                    && context.filing.is_some()
                {
                    assert!(context.waiting_content.is_some());
                    assert!(context.barrier.pending_author().is_none());
                    assert!(
                        member
                            .filings
                            .iter()
                            .all(|file| !matches!(file, s::StateFrame::FilePair { .. }))
                    );
                    if mode.contains("-author-") {
                        use crate::flow::recording::{RecordProgress, RecordPurpose};
                        super::super::failed::tests::fill_trace(context, clock);
                        // Pause the normal author path at the recorder's real pending
                        // boundary, without fabricating a ticket or Recorded proof.
                        let event = context.waiting_content.take().unwrap().into_event();
                        context.author_turn = Some((event.kind, event.turn.clone()));
                        let queued = context.barrier.queue_authored_with_clock(
                            event,
                            RecordPurpose::Content,
                            &context.author,
                            1,
                            &mut context.inbox,
                            &mut context.budget,
                            &mut || clock.now(),
                        );
                        assert!(matches!(queued.progress, RecordProgress::Pending));
                        assert_eq!(
                            queued.ticket,
                            context.barrier.pending_author().map(|author| author.ticket)
                        );
                    }
                    break;
                }
                if repair_filing && mode.contains("-measurement-") && member.held_file.is_some() {
                    // Finish the earlier send observation while the actual receipt
                    // remains at the peer, then put its measurement under pressure.
                    for _ in 0..8 {
                        context.reduce_flow(&mut CircuitIo(clock)).unwrap();
                    }
                    assert!(context.barrier.pending().is_none());
                    super::super::failed::tests::fill_trace(context, clock);
                    member.hold_kind = None;
                    member.step(clock.now().unwrap(), "ready");
                    while context.barrier.pending().is_none() {
                        assert!(
                            Instant::now() < limit,
                            "actual filing measurement did not become pending"
                        );
                        context
                            .pump_once(&mut PumpPorts {
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
                                io: &mut CircuitIo(clock),
                            })
                            .unwrap();
                        context.reduce_flow(&mut CircuitIo(clock)).unwrap();
                    }
                    assert_eq!(
                        context.barrier.pending().unwrap().envelope.kind,
                        weaver_trace::serving::Kind::StateStoreMeasurement
                    );
                    break;
                }
                if filing_pressure && context.barrier.pending().is_some() {
                    assert_eq!(
                        context.barrier.pending().unwrap().envelope.kind,
                        weaver_trace::serving::Kind::StateFlow
                    );
                    member.step(clock.now().unwrap(), "ready");
                    assert!(
                        member.held_file.is_some(),
                        "the original complete send predates expiry"
                    );
                    break;
                }
                if repair_filing && mode.contains("-pending-") && member.held_file.is_some() {
                    assert!(context.filing.is_some());
                    assert_eq!(context.flow.state().outstanding().len(), 1);
                    if mode.contains("-partial-") {
                        let partial = member.send_held_filing_prefix();
                        context
                            .pump_once(&mut PumpPorts {
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
                                io: &mut CircuitIo(clock),
                            })
                            .unwrap();
                        assert_eq!(
                            context.flow.state().retained_received_bytes(),
                            partial.as_slice()
                        );
                        assert!(context.flow.state().held_measurement().is_none());
                    }
                    break;
                }
                if repair_filing
                    && mode.contains("-queued-")
                    && context.filing.is_some()
                    && context.flow.ledger().facts().next().is_some()
                    && context.flow.state().outstanding().is_empty()
                {
                    assert_eq!(context.flow.ledger().facts().count(), 1);
                    assert!(
                        member
                            .filings
                            .iter()
                            .all(|file| !matches!(file, s::StateFrame::FilePair { .. }))
                    );
                    break;
                }
                if repair_filing
                    && mode.contains("-unsent-")
                    && context
                        .flow
                        .state()
                        .outstanding()
                        .first()
                        .is_some_and(|work| {
                            work.operation == s::StateOperation::FilePair && work.bytes_sent == 0
                        })
                {
                    assert!(context.filing.is_some());
                    assert!(
                        member
                            .filings
                            .iter()
                            .all(|file| !matches!(file, s::StateFrame::FilePair { .. }))
                    );
                    break;
                }
                if progress == BootPairsProgress::Complete {
                    assert!(!proposed, "fixture must retain the unstarted terminal");
                    assert!(!repair_filing, "fixture must retain the actual filing");
                    break;
                }
                member.step(clock.now().unwrap(), "ready");
                if progress == BootPairsProgress::Pending {
                    context
                        .pump_once(&mut PumpPorts {
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
                            io: &mut CircuitIo(clock),
                        })
                        .unwrap();
                }
                std::thread::sleep(Duration::from_millis(1));
            }
        }
    }
    if mode.starts_with("expired-startup-admission-") && !mode.contains("-before-fork-") {
        let EnterState::Startup(owner) = guard.0.as_mut().unwrap().state.as_mut().unwrap() else {
            panic!("original initial startup owner");
        };
        // SAFETY: the actual child remains owned by the fixture's reap guard.
        unsafe { owner.failure_test_stage_classifier(&mut CircuitIo(clock)) };
        if !mode.contains("-unsent-") {
            let context = context(guard.0.as_mut().unwrap());
            while context.flow.held_admission().is_none()
                && context
                    .flow
                    .starting_classifier()
                    .unwrap()
                    .test_partial_bytes()
                    == 0
            {
                assert!(Instant::now() < limit, "held admission did not arrive");
                context
                    .flow
                    .advance_classifier_admission(&mut FlowRecording {
                        barrier: &mut context.barrier,
                        inbox: &mut context.inbox,
                        budget: &mut context.budget,
                        author: &context.author,
                    })
                    .unwrap();
                std::thread::sleep(Duration::from_millis(1));
            }
            use crate::flow::ste100_socket::AdmissionPoll;
            if mode.contains("-ready-") {
                assert!(matches!(
                    context.flow.held_admission(),
                    Some(AdmissionPoll::Ready(_))
                ));
            } else if mode.contains("-refused-") {
                assert!(matches!(
                    context.flow.held_admission(),
                    Some(AdmissionPoll::Refused(_))
                ));
            } else {
                assert!(mode.contains("-partial-"));
                assert!(context.flow.held_admission().is_none());
                assert_eq!(
                    context
                        .flow
                        .starting_classifier()
                        .unwrap()
                        .test_partial_bytes(),
                    65_510
                );
            }
        }
    }
    if mode.contains("-template-completion-") {
        // Reduce the actual Recorded classifier proof without letting the boot
        // driver consume it into the transcript in the same call. This exercises
        // the intermediate coordinator owner, with no fabricated result/proof.
        let context = context(guard.0.as_mut().unwrap());
        while context.flow.operation_completion().is_none() {
            assert!(
                Instant::now() < limit,
                "Recorded classifier did not release"
            );
            context.reduce_flow(&mut CircuitIo(clock)).unwrap();
        }
        assert!(matches!(
            context.flow.operation_completion(),
            Some(OperationCompletion::Check(_))
        ));
        assert_eq!(context.template_transcript().unwrap().accepted_count(), 0);
    }
    if mode.contains("-template-foreign-") {
        use crate::flow::recording::{RecordProgress, RecordPurpose};
        let context = context(guard.0.as_mut().unwrap());
        let author = context.barrier.pending_author().unwrap();
        let event = author.event.clone();
        let ticket = author.ticket;
        assert!(context.barrier.abandon_boot_author(ticket));
        assert!(matches!(
            context.barrier.submit_authored_with_clock(
                event,
                RecordPurpose::Observation,
                &context.author,
                1,
                &mut context.inbox,
                &mut context.budget,
                &mut || clock.now()
            ),
            RecordProgress::Pending
        ));
    }
    guard.0 = Some(
        guard
            .0
            .take()
            .unwrap()
            .expired_handoff()
            .err()
            .expect("not expired"),
    );
    if read_target && mode.contains("-measurement-") {
        super::super::failed::tests::fill_trace(context(guard.0.as_mut().unwrap()), clock);
        member.hold_control = None;
        member.step(clock.now().unwrap(), "ready");
        while context(guard.0.as_mut().unwrap())
            .barrier
            .pending()
            .is_none()
        {
            assert!(
                Instant::now() < limit,
                "read measurement did not reach writer pressure"
            );
            // SAFETY: all original child owners remain in the fixture guard.
            assert_eq!(
                unsafe {
                    guard
                        .0
                        .as_mut()
                        .unwrap()
                        .advance_with_io(&mut CircuitIo(clock))
                },
                FoundationEnterProgress::Pending
            );
            std::thread::sleep(Duration::from_millis(1));
        }
        assert_eq!(
            context(guard.0.as_mut().unwrap())
                .barrier
                .pending()
                .unwrap()
                .envelope
                .kind,
            weaver_trace::serving::Kind::StateStoreMeasurement
        );
        // The same trace kind alone is not this read's authority. Exercise
        // foreign purpose and payload under real writer pressure, then restore
        // the actual channel measurement before taking the final evidence.
        let context = context(guard.0.as_mut().unwrap());
        let original = context.barrier.pending_author().unwrap().event.clone();
        let measurement = context.flow.state().held_measurement().unwrap().clone();
        let mut foreign = measurement.clone();
        foreign.observation.request_id = (foreign.observation.request_id.0 + 1).into();
        let mut foreign_event = original.clone();
        foreign_event.payload = Some(weaver_trace::serving::Payload::StateStoreMeasurement(
            Box::new(crate::flow::mirror::measurement(&foreign)),
        ));
        use crate::flow::recording::{RecordProgress, RecordPurpose};
        for (event, purpose, valid) in [
            (original.clone(), RecordPurpose::Observation, false),
            (foreign_event, RecordPurpose::State(foreign), false),
            (original, RecordPurpose::State(measurement), true),
        ] {
            let ticket = context.barrier.pending_author().unwrap().ticket;
            assert!(context.barrier.abandon_boot_author(ticket));
            assert!(matches!(
                context.barrier.submit_authored_with_clock(
                    event,
                    purpose,
                    &context.author,
                    1,
                    &mut context.inbox,
                    &mut context.budget,
                    &mut || clock.now()
                ),
                RecordProgress::Pending
            ));
            assert_eq!(
                context.read_rollback_permit().is_ok(),
                valid,
                "read measurement must match exact owner"
            );
        }
    }
    if mode == "expired-startup-admit-pending-complete" {
        let EnterState::Residency(owner) = guard.0.as_mut().unwrap().state.as_mut().unwrap() else {
            panic!("actual Admit owner");
        };
        owner.failure_test_stage_admit();
    }
    if mode == "expired-startup-open-unsent-complete" {
        let EnterState::Boot(owner) = guard.0.as_mut().unwrap().state.as_mut().unwrap() else {
            panic!("actual Open owner");
        };
        owner.failure_test_stage_open(clock);
    }
    if mode == "expired-startup-open-record-complete" {
        super::super::failed::tests::fill_trace(context(guard.0.as_mut().unwrap()), clock);
        while !context(guard.0.as_mut().unwrap()).boot_owns_pending_author() {
            assert!(
                Instant::now() < limit,
                "Open placement did not reach writer pressure"
            );
            // SAFETY: all original child owners remain in the fixture guard.
            assert_eq!(
                unsafe {
                    guard
                        .0
                        .as_mut()
                        .unwrap()
                        .advance_with_io(&mut CircuitIo(clock))
                },
                FoundationEnterProgress::Pending
            );
            std::thread::sleep(Duration::from_millis(1));
        }
        assert!(context(guard.0.as_mut().unwrap()).boot_open_raw().is_some());
    }
    let enter = context(guard.0.as_mut().unwrap())
        .deadlines
        .get(PumpDeadline::Enter)
        .unwrap();
    let origin = Arc::clone(&context(guard.0.as_mut().unwrap()).read_origin);
    if summary_boundary::is_mode(mode) {
        summary_boundary::expire(context(guard.0.as_mut().unwrap()), clock, mode);
    }
    let progress = if mode == "expired-startup-installed-unsent-complete" {
        // The original owner's first sample checks live Enter. Production then
        // stages its original Raise before the common pump checks live again.
        // Expire that second sample, before any transport send is attempted.
        struct BeforeRaiseSend<'a> {
            clock: &'a mut FakePumpIo,
            end: u64,
            samples: usize,
        }
        impl PumpIo for BeforeRaiseSend<'_> {
            fn now(&mut self) -> std::io::Result<u64> {
                self.samples += 1;
                if self.samples == 2 {
                    self.clock.advance_to(self.end)?;
                }
                self.clock.now()
            }
            fn wait(
                &mut self,
                _: &[WaitInterest<'_>],
                _: Option<u64>,
            ) -> std::io::Result<crate::flow::clock::WaitOutcome> {
                panic!("original Enter expiry must precede the first Raise send or wait");
            }
        }
        let mut io = BeforeRaiseSend {
            clock,
            end: enter.deadline_ns(),
            samples: 0,
        };
        // SAFETY: the original fixture retains the serving thread and child.
        let progress = unsafe { guard.0.as_mut().unwrap().advance_with_io(&mut io) };
        assert_eq!(
            io.samples, 2,
            "pin the production stage/first-send boundary"
        );
        progress
    } else {
        clock.advance_to(enter.deadline_ns()).unwrap();
        // SAFETY: as above. The real stage observes exact-deadline expiry itself.
        unsafe {
            guard
                .0
                .as_mut()
                .unwrap()
                .advance_with_io(&mut CircuitIo(clock))
        }
    };
    assert_eq!(progress, FoundationEnterProgress::Stopped);
    assert!(
        context(guard.0.as_mut().unwrap()).failure().is_none(),
        "expiry is not failure"
    );
    assert!(context(guard.0.as_mut().unwrap()).flow.halt().is_none());
    let handoff = guard
        .0
        .take()
        .unwrap()
        .expired_handoff()
        .ok()
        .expect("actual expired startup");
    assert!(Arc::ptr_eq(&handoff.identity.origin, &origin));
    assert_eq!(handoff.identity.enter, enter);
    handoff
}

#[allow(clippy::too_many_arguments)]
pub(in crate::flow::context::worker_startup) fn finish(
    run: Box<FoundationRun>,
    mut handoff: Box<FoundationExpiredEnter>,
    enter_peer: &OwnedFd,
    member: &mut Member,
    reader: &mut UnixStream,
    clock: &mut FakePumpIo,
    path: &Path,
    original: &[u8],
    mode: &str,
) {
    let _reader = PanicReader(reader.try_clone().unwrap());
    if mode.ends_with("-release-early") {
        let until = Instant::now() + Duration::from_secs(2);
        while !path.parent().unwrap().join("release-early").exists() {
            assert!(Instant::now() < until, "early Release fixture did not send");
            std::thread::sleep(Duration::from_millis(1));
        }
    }
    assert_eq!(handoff.held.original_enter().unwrap().wire(), original);
    let address = handoff.held.original_enter().unwrap().wire().as_ptr();
    assert!(path.exists());
    assert!(
        packet(enter_peer).is_none(),
        "expiry produces no lifecycle answer"
    );
    if mode.ends_with("-foreign") {
        let mut foreign =
            crate::flow::context::tests::Fixture::new(crate::flow::context::tests::config());
        let mut context =
            crate::flow::context::failed_enter_driver::tests::take_context(&mut foreign);
        let resources = context.failed_enter_resources().unwrap();
        let foreign = Box::new(FoundationRun {
            context,
            resources,
            listener: None,
            original: None,
        });
        let (foreign, returned) = foreign
            .expired_rollback(handoff)
            .err()
            .expect("foreign run");
        assert!(foreign.context.failure().is_none());
        drop(foreign);
        handoff = returned;
        assert_eq!(
            handoff.held.original_enter().unwrap().wire().as_ptr(),
            address
        );
    }
    let mut guard = Guard(Some(run.expired_rollback(handoff).ok().expect("exact run")));
    let owner = guard.0.as_mut().unwrap();
    assert_eq!(owner.process_exit(), None);
    assert_eq!(
        owner.progress(),
        FoundationExpiredRollbackProgress::AwaitRollback
    );
    assert_eq!(
        owner.advance_with_io(clock),
        FoundationExpiredRollbackProgress::AwaitRollback
    );
    assert!(
        owner
            .run
            .context
            .deadlines
            .get(PumpDeadline::Leave)
            .is_none()
    );
    let requests = member.requests.clone();
    let unsent_gate = matches!(
        mode,
        "expired-startup-installed-complete" | "expired-startup-installed-unsent-complete"
    )
    .then(|| {
        let HeldStartup::Gate(held) = owner.handoff.held.as_ref() else {
            panic!("original unraised gate account");
        };
        let arms = owner.handoff.gate.as_ref().unwrap();
        assert!(held.reply.is_none() && held.returned.is_none());
        let queued = mode == "expired-startup-installed-unsent-complete";
        assert_eq!(held.send.is_some(), queued);
        assert_eq!(arms.port.is_some(), queued);
        assert_eq!(arms.child.is_some(), queued);
        if let Some(send) = &held.send {
            assert!(!send.sent() && send.sent_at().is_none());
            assert!(send.belongs_to(arms.port.as_ref().unwrap()));
            assert!(send.matches_exchange(&held.raise.exchange));
        }
        let outgoing = arms
            .port
            .as_ref()
            .and_then(|port| port.test_outgoing_packet())
            .map(|(bytes, offset)| {
                assert_eq!(offset, 0, "the original Raise is wholly unsent");
                assert_eq!(
                    serde_json::from_slice::<OrganEnvelope>(bytes).unwrap(),
                    held.raise
                );
                (bytes.to_vec(), bytes.as_ptr() as usize, offset)
            });
        assert_eq!(outgoing.is_some(), queued);
        (serde_json::to_vec(&held.raise).unwrap(), outgoing)
    });
    if gate_failure::before_leave(mode) {
        gate_failure::finish(
            &mut guard, member, reader, clock, path, enter_peer, &requests, mode,
        );
        return;
    }
    let initial_admission = mode.starts_with("expired-startup-classifying")
        || mode.starts_with("expired-startup-admission-");
    let admission_evidence = owner
        .run
        .context
        .flow
        .starting_classifier()
        .map(|port| port.test_evidence());
    let admission_completion = owner.run.context.flow.held_admission().cloned();
    let classifier_started = owner.run.resources.classifier_child().is_some();
    let spu_started = owner.run.resources.spu_child().is_some();
    if initial_admission {
        assert!(
            !spu_started,
            "initial classifier admission precedes SPU fork"
        );
        assert_eq!(classifier_started, !mode.contains("-before-fork-"));
        let HeldStartup::Initial(held) = owner.handoff.held.as_ref() else {
            panic!("retain initial bootstrap");
        };
        if mode.starts_with("expired-startup-classifying") || mode.contains("-partial-") {
            assert_eq!(
                std::fs::read(path.parent().unwrap().join("classifier-bootstrap")).unwrap(),
                serde_json::to_vec(&held.bootstrap).unwrap()
            );
        }
        assert!(owner.run.context.flow.standing_classifier().is_none());
    }
    let summary_boundary_evidence = summary_boundary::evidence(&owner.run.context, member, mode);
    let summary_evidence = owner
        .run
        .context
        .plan_summary
        .as_ref()
        .filter(|_| owner.run.context.read.is_none())
        .map(|work| work.expired_test_evidence());
    let summary_cache = summary_evidence
        .as_ref()
        .map(|_| owner.run.context.expired_summary_cache_evidence());
    let extracted_summary = match owner.handoff.held.as_ref() {
        HeldStartup::Boot(boot) => boot.summary.as_ref().map(|summary| format!("{summary:?}")),
        _ => None,
    };
    let boot_pair_evidence = owner
        .run
        .context
        .boot_pairs
        .as_ref()
        .map(|work| work.expired_test_evidence());
    let filing_author = owner.run.context.test_boot_filing_author();
    let filing_trace = owner.run.context.test_boot_filing_trace();
    let filing_record = filing_author.as_ref().and_then(|_| {
        owner.run.context.barrier.pending().map(|event| {
            (
                serde_json::to_vec(event).unwrap(),
                owner.run.context.barrier.pending_author().unwrap().ticket,
            )
        })
    });
    let filing_pending =
        filing_author.is_some() && !mode.contains("-unstarted-") && !mode.contains("-settled-");
    let filing_flow = filing_author
        .as_ref()
        .map(|_| owner.run.context.flow.test_boot_filing_evidence());
    let filing_channel =
        filing_pending.then(|| owner.run.context.flow.test_boot_filing_channel().unwrap());
    let filing_intake = filing_author.as_ref().map(|_| {
        format!(
            "{:?}",
            (&owner.run.context.inbox, &owner.run.context.budget)
        )
    });
    let boot_pair_filings = boot_pair_evidence.as_ref().map(|_| {
        member
            .filings
            .iter()
            .filter_map(|file| match file {
                s::StateFrame::FilePair { filing, .. } => Some(filing.clone()),
                _ => None,
            })
            .collect::<Vec<_>>()
    });
    if mode == "expired-startup-repair-filed-complete" {
        let [filing] = boot_pair_filings.as_ref().unwrap().as_slice() else {
            panic!("one actual repair filing before expiry");
        };
        let s::PairRecord::Ended(ended) = &filing.pair else {
            panic!("actual abandonment terminal");
        };
        assert_eq!(ended.call_id, member.boot_pairs[0].call_id);
        assert_eq!(ended.loaded_from, owner.run.context.open_start);
        assert_eq!(ended.fate, s::TerminalPairFate::Failed);
        assert_eq!(ended.reason, Some(s::PairFailure::AbandonedAtLoad {}));
        assert!(ended.execution.is_none() && ended.outcome.is_none());
        let [terminal] = owner
            .run
            .context
            .boot_pairs
            .as_ref()
            .unwrap()
            .expired_test_terminals()
        else {
            panic!("retain the actual repair receipt");
        };
        let mut dispositions = terminal.receipts().dispositions();
        let Some(crate::flow::ledger::SourceDisposition::Committed(source)) = dispositions.next()
        else {
            panic!("repair filing must be committed");
        };
        assert!(dispositions.next().is_none());
        let crate::flow::ledger::LedgerFile::Pair(file) = source.file() else {
            panic!("retain the exact PairFile");
        };
        assert_eq!(
            serde_json::to_vec(file).unwrap(),
            serde_json::to_vec(filing).unwrap()
        );
        assert!(
            matches!(source.receipt(), s::StateFrame::Filed { block_id, .. } if block_id == source.source())
        );
        assert_eq!(source.source().run, filing.source.run);
        assert_eq!(source.source().sequence, filing.source.sequence);
        assert_eq!(source.source().block.0, 0);
    }
    let template_target = mode.contains("-template-");
    let template_request = template_target
        .then(|| owner.run.context.flow.classifier().outstanding().cloned())
        .flatten();
    let template_reply = template_target
        .then(|| {
            owner
                .run
                .context
                .flow
                .classifier()
                .held()
                .cloned()
                .or_else(|| match owner.run.context.flow.operation_completion() {
                    Some(OperationCompletion::Check(record)) => Some(record.reply().clone()),
                    _ => None,
                })
        })
        .flatten();
    let template_extra_recorded = usize::from(
        template_target
            && (owner
                .run
                .context
                .flow
                .pending_classifier_recorded()
                .is_some()
                || matches!(
                    owner.run.context.flow.operation_completion(),
                    Some(OperationCompletion::Check(_))
                )),
    );
    let template_records = template_target.then(|| {
        owner
            .run
            .context
            .template_transcript()
            .unwrap()
            .records()
            .iter()
            .map(|record| {
                (
                    record.operation().clone(),
                    record.reply().clone(),
                    record.result_source().clone(),
                )
            })
            .collect::<Vec<_>>()
    });
    let template_record = template_target
        .then(|| {
            owner.run.context.barrier.pending().map(|event| {
                (
                    serde_json::to_vec(event).unwrap(),
                    owner.run.context.barrier.pending_author().unwrap().ticket,
                )
            })
        })
        .flatten();
    let read_evidence = owner.run.context.read.as_ref().map(|read| {
        (
            read.plan_bound(),
            read.plan_expected().cloned(),
            read.acquired_snapshot(),
            read.assembled_bytes().to_vec(),
            read.plan_ticket(),
            read.plan_recorded(),
        )
    });
    let pending_read = read_evidence.is_some();
    let pending_read_record = pending_read
        .then(|| {
            owner.run.context.barrier.pending().map(|event| {
                (
                    serde_json::to_vec(event).unwrap(),
                    owner
                        .run
                        .context
                        .barrier
                        .pending_author()
                        .map(|author| author.ticket),
                )
            })
        })
        .flatten();
    if pending_read && summary_boundary::is_mode(mode) {
        assert!(member.held_control.is_none());
        let read = owner.run.context.read.as_ref().unwrap();
        assert!(read.plan_expected().is_none());
        assert!(read.plan_recorded().is_some());
    } else if pending_read {
        assert_eq!(
            member.held_control.is_none(),
            mode.contains("-measurement-")
        );
        let read = owner.run.context.read.as_ref().unwrap();
        assert!(read.plan_expected().is_some());
        assert_eq!(
            read.acquired_snapshot().is_some(),
            mode.contains("-cursor-")
        );
        assert_eq!(
            !read.assembled_bytes().is_empty(),
            mode.contains("-cursor-")
        );
    }
    let open_request = owner.run.context.boot_open_request().cloned();
    let open_raw = owner.run.context.boot_open_raw().map(<[u8]>::to_vec);
    let pending_open = owner.run.context.boot_owns_pending_author().then(|| {
        (
            serde_json::to_vec(owner.run.context.barrier.pending().unwrap()).unwrap(),
            owner.run.context.barrier.pending_author().unwrap().ticket,
        )
    });
    if mode.ends_with("-wait") {
        let path = path.to_owned();
        // Startup has already forked its children. This thread owns only the
        // root-side dial which wakes the real blocking listener wait.
        let wake = std::thread::spawn(move || {
            std::thread::sleep(Duration::from_millis(80));
            dial(&path)
        });
        let began = Instant::now();
        assert_eq!(
            owner.wait_with_io(&mut MonotonicIo),
            FoundationExpiredRollbackProgress::AwaitRollback
        );
        assert!(
            began.elapsed() >= Duration::from_millis(50),
            "await-root spun instead of blocking"
        );
        let peer = wake.join().unwrap();
        clock.advance_to(owner.run.context.last_ns).unwrap();
        owner.advance_with_io(clock);
        let read = owner.dial.as_ref().unwrap().read;
        clock.advance_to(read.deadline_ns()).unwrap();
        owner.advance_with_io(clock);
        drop(peer);
        assert!(owner.dial.is_none());
        assert!(owner.leave.is_none());
        assert_eq!(member.requests, requests);
    }
    if mode.ends_with("-observe") {
        for directive in [
            LifecycleDirective::Observe,
            LifecycleDirective::Stop {
                cause: weaver_types::Cause { uid: 99 },
            },
        ] {
            let peer = dial(path);
            let envelope = OrganEnvelope {
                exchange: weaver_types::ExchangeId {
                    opener: Opener::Admin,
                    ordinal: 18,
                },
                position: Position::Open,
                payload: Payload::Directive(directive),
            };
            send(
                peer.as_raw_fd(),
                &serde_json::to_vec(&envelope).unwrap(),
                MsgFlags::MSG_NOSIGNAL,
            )
            .unwrap();
            let mut answered = false;
            for _ in 0..8 {
                assert_eq!(
                    owner.advance_with_io(clock),
                    FoundationExpiredRollbackProgress::AwaitRollback
                );
                if let Some(answer) = packet(&peer) {
                    assert!(matches!(
                        answer.payload,
                        Payload::Answer(LifecycleAnswer::InTransition { .. })
                            | Payload::Refusal(LifecycleRefusal::OutOfOrder)
                    ));
                    assert_eq!(answer.exchange, envelope.exchange);
                    answered = true;
                    break;
                }
            }
            assert!(answered, "secondary lifecycle peer has an actual answer");
            assert!(owner.leave.is_none());
            assert!(
                owner
                    .run
                    .context
                    .deadlines
                    .get(PumpDeadline::Leave)
                    .is_none()
            );
        }
    }
    if mode.ends_with("-silent") {
        let peer = dial(path);
        owner.advance_with_io(clock);
        let read = owner.dial.as_ref().unwrap().read;
        clock.advance_to(read.deadline_ns()).unwrap();
        assert_eq!(
            owner.advance_with_io(clock),
            FoundationExpiredRollbackProgress::AwaitRollback
        );
        assert!(owner.dial.is_none());
        assert!(owner.leave.is_none());
        drop(peer);
    }
    let peer = dial(path);
    let directive = OrganEnvelope {
        exchange: weaver_types::ExchangeId {
            opener: Opener::Admin,
            ordinal: 21,
        },
        position: Position::Open,
        payload: Payload::Directive(LifecycleDirective::Leave {
            cause: weaver_types::Cause { uid: 321 },
            forced: true,
            rollback: true,
        }),
    };
    send(
        peer.as_raw_fd(),
        &serde_json::to_vec(&directive).unwrap(),
        MsgFlags::MSG_NOSIGNAL,
    )
    .unwrap();
    if owner.before_load {
        assert_eq!(
            owner.advance_with_io(clock),
            FoundationExpiredRollbackProgress::AwaitRollback
        );
        let answer = packet(&peer).expect("before-load rollback has an actual response");
        assert_eq!(answer.exchange, directive.exchange);
        assert_eq!(answer.position, Position::Close);
        assert!(matches!(
            answer.payload,
            Payload::Refusal(LifecycleRefusal::OutOfOrder)
        ));
        assert!(owner.leave.is_none());
        assert!(
            owner
                .run
                .context
                .deadlines
                .get(PumpDeadline::Leave)
                .is_none()
        );
        assert!(owner.run.context.failure().is_none());
        assert!(owner.run.context.flow.halt().is_none());
        assert!(owner.run.resources.spu_child().is_none());
        assert!(owner.run.resources.classifier_child().is_none());
        assert_eq!(
            owner.original.as_ref().unwrap().retained_wire().as_ptr(),
            address
        );
        assert_eq!(owner.original.as_ref().unwrap().retained_wire(), original);
        assert!(owner.retired.as_ref().unwrap().state.is_some());
        assert!(owner.run.context.flow.standing_state().is_none());
        let mut bytes = vec![];
        reader.set_nonblocking(true).unwrap();
        let until = Instant::now() + Duration::from_secs(2);
        loop {
            match reader.read_to_end(&mut bytes) {
                Ok(_) => break,
                Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => {
                    assert!(
                        Instant::now() < until,
                        "unadmitted trace descriptor did not close"
                    );
                    std::thread::sleep(Duration::from_millis(1));
                }
                Err(error) => panic!("unadmitted trace: {error}"),
            }
        }
        assert!(bytes.is_empty(), "no record is manufactured before Load");
        return;
    }
    assert_eq!(
        owner.advance_with_io(clock),
        FoundationExpiredRollbackProgress::Accepted,
        "{:?}",
        owner.detail()
    );
    assert_eq!(owner.original.as_ref().unwrap().retained_wire(), original);
    assert_eq!(
        owner.original.as_ref().unwrap().retained_wire().as_ptr(),
        address
    );
    assert_eq!(
        owner
            .dial
            .as_ref()
            .unwrap()
            .message
            .as_ref()
            .unwrap()
            .envelope,
        directive
    );
    let leave = owner.leave.unwrap();
    let original_enter = owner.handoff.identity.enter;
    assert_eq!(leave.began_ns(), clock.now().unwrap());
    assert_eq!(
        leave.deadline_ns(),
        leave.began_ns() + weaver_types::LEAVE_BOUND_MS * 1_000_000
    );
    assert_eq!(
        owner.run.context.deadlines.get(PumpDeadline::Leave),
        Some(leave)
    );
    assert_eq!(
        owner.run.context.deadlines.get(PumpDeadline::Enter),
        Some(owner.handoff.identity.enter)
    );
    assert!(owner.run.context.failure().is_none());
    assert!(owner.run.context.flow.halt().is_none());
    assert!(packet(enter_peer).is_none());
    assert!(
        packet(&peer).is_none(),
        "acceptance alone cannot grant Left"
    );
    member.step(clock.now().unwrap(), "ready");
    assert_eq!(
        member.requests, requests,
        "no new boot work during rollback admission"
    );
    if gate_failure::is_mode(mode) {
        gate_failure::finish(
            &mut guard, member, reader, clock, path, &peer, &requests, mode,
        );
        return;
    }
    if gate_last_word::is_mode(mode) {
        gate_last_word::finish(
            &mut guard, member, reader, clock, path, &peer, &directive, &requests, mode,
        );
        return;
    }
    if gate_cases::is_mode(mode) {
        gate_cases::finish(
            owner, member, reader, clock, path, &peer, &directive, &requests, mode,
        );
        return;
    }
    if admission_evidence.is_some() {
        let context = &owner.run.context;
        for (origin, enter, leave) in [
            (Arc::new(()), original_enter, leave),
            (
                Arc::clone(&context.read_origin),
                AbsoluteBound::from_absolute(
                    original_enter.began_ns() + 1,
                    original_enter.deadline_ns(),
                )
                .unwrap(),
                leave,
            ),
            (
                Arc::clone(&context.read_origin),
                original_enter,
                AbsoluteBound::from_absolute(leave.began_ns() + 1, leave.deadline_ns()).unwrap(),
            ),
        ] {
            assert!(
                !context.can_detach_expired_admission(&ExpiredStartupAccountPermit {
                    origin,
                    enter,
                    leave
                }),
                "foreign rollback cannot detach admission"
            );
        }
        assert!(
            context.can_detach_expired_admission(&ExpiredStartupAccountPermit {
                origin: Arc::clone(&context.read_origin),
                enter: original_enter,
                leave,
            })
        );
    }
    if mode == "expired-startup-admit-sent-complete" {
        assert!(
            matches!(owner.handoff.held.as_ref(), HeldStartup::Residency(held) if held.reply.is_none())
        );
        std::fs::write(path.parent().unwrap().join("allow-admit-reply"), b"allowed").unwrap();
    }
    if mode.starts_with("expired-startup-open-") {
        let enter = owner.handoff.identity.enter;
        let context = &owner.run.context;
        let decode = owner.run.resources.decode_present_for_serving().unwrap();
        for (origin, enter, leave) in [
            (Arc::new(()), enter, leave),
            (
                Arc::clone(&context.read_origin),
                AbsoluteBound::from_absolute(enter.began_ns() + 1, enter.deadline_ns()).unwrap(),
                leave,
            ),
            (
                Arc::clone(&context.read_origin),
                enter,
                AbsoluteBound::from_absolute(leave.began_ns() + 1, leave.deadline_ns()).unwrap(),
            ),
        ] {
            let permit = ExpiredStartupAccountPermit {
                origin,
                enter,
                leave,
            };
            assert!(
                !context.can_detach_expired_boot_open(&permit, decode),
                "foreign rollback cannot detach the boot account"
            );
        }
        let permit = ExpiredStartupAccountPermit {
            origin: Arc::clone(&context.read_origin),
            enter,
            leave,
        };
        assert!(context.can_detach_expired_boot_open(&permit, decode));
        let (foreign, _peer) = crate::channel::DecodeChannel::pair().unwrap();
        let foreign = DecodePort::from_channel(foreign).unwrap();
        assert!(
            !context.can_detach_expired_boot_open(&permit, &foreign),
            "retain the exact decode port"
        );
        assert!(context.failure.is_none() && context.flow.halt().is_none());
    }
    if template_target {
        let context = &owner.run.context;
        for (origin, enter, leave) in [
            (Arc::new(()), original_enter, leave),
            (
                Arc::clone(&context.read_origin),
                AbsoluteBound::from_absolute(
                    original_enter.began_ns() + 1,
                    original_enter.deadline_ns(),
                )
                .unwrap(),
                leave,
            ),
            (
                Arc::clone(&context.read_origin),
                original_enter,
                AbsoluteBound::from_absolute(leave.began_ns() + 1, leave.deadline_ns()).unwrap(),
            ),
        ] {
            assert!(
                !context.can_detach_expired_templates(&ExpiredStartupAccountPermit {
                    origin,
                    enter,
                    leave
                })
            );
        }
        assert!(
            context.can_detach_expired_templates(&ExpiredStartupAccountPermit {
                origin: Arc::clone(&context.read_origin),
                enter: original_enter,
                leave,
            })
        );
    }
    if boot_pair_evidence.is_some() {
        let context = &mut owner.run.context;
        if context.filing.is_some() {
            context.test_boot_filing_identity_guards();
        }
        for (origin, enter, leave) in [
            (Arc::new(()), original_enter, leave),
            (
                Arc::clone(&context.read_origin),
                AbsoluteBound::from_absolute(
                    original_enter.began_ns() + 1,
                    original_enter.deadline_ns(),
                )
                .unwrap(),
                leave,
            ),
            (
                Arc::clone(&context.read_origin),
                original_enter,
                AbsoluteBound::from_absolute(leave.began_ns() + 1, leave.deadline_ns()).unwrap(),
            ),
        ] {
            assert!(
                !context.can_detach_expired_boot_pairs(&ExpiredStartupAccountPermit {
                    origin,
                    enter,
                    leave,
                }),
                "foreign rollback cannot detach boot repair"
            );
        }
        let permit = ExpiredStartupAccountPermit {
            origin: Arc::clone(&context.read_origin),
            enter: original_enter,
            leave,
        };
        assert!(context.can_detach_expired_boot_pairs(&permit));
        let start = context.open_start.take();
        assert!(
            !context.can_detach_expired_boot_pairs(&permit),
            "retain the actual StartIdentity"
        );
        context.open_start = start;
        let head = context.boot_head.take();
        assert!(
            !context.can_detach_expired_boot_pairs(&permit),
            "retain the original checked boot head"
        );
        context.boot_head = head;
        let decision = context.deadlines.disarm(PumpDeadline::Decision);
        context
            .deadlines
            .arm(PumpDeadline::Decision, leave)
            .unwrap();
        assert!(
            !context.can_detach_expired_boot_pairs(&permit),
            "foreign Decision wait cannot become cleanup"
        );
        context.deadlines.disarm(PumpDeadline::Decision);
        if let Some(decision) = decision {
            context
                .deadlines
                .arm(PumpDeadline::Decision, decision)
                .unwrap();
        }
        assert!(context.can_detach_expired_boot_pairs(&permit));
    }
    if pending_read {
        let context = &mut owner.run.context;
        for (origin, enter, leave) in [
            (Arc::new(()), original_enter, leave),
            (
                Arc::clone(&context.read_origin),
                AbsoluteBound::from_absolute(
                    original_enter.began_ns() + 1,
                    original_enter.deadline_ns(),
                )
                .unwrap(),
                leave,
            ),
            (
                Arc::clone(&context.read_origin),
                original_enter,
                AbsoluteBound::from_absolute(leave.began_ns() + 1, leave.deadline_ns()).unwrap(),
            ),
        ] {
            assert!(
                !context.can_park_expired_startup_read(&ExpiredStartupAccountPermit {
                    origin,
                    enter,
                    leave
                }),
                "foreign rollback cannot dispose this read"
            );
        }
        let permit = ExpiredStartupAccountPermit {
            origin: Arc::clone(&context.read_origin),
            enter: original_enter,
            leave,
        };
        assert!(context.can_park_expired_startup_read(&permit));
        let original = context.deadlines.disarm(PumpDeadline::StateControl);
        let read_bound = context.read.as_ref().unwrap().plan_bound();
        let changed =
            AbsoluteBound::from_absolute(read_bound.began_ns() + 1, read_bound.deadline_ns())
                .unwrap();
        context
            .deadlines
            .arm(PumpDeadline::StateControl, changed)
            .unwrap();
        assert!(
            !context.can_park_expired_startup_read(&permit),
            "read wait must remain exact"
        );
        context.deadlines.disarm(PumpDeadline::StateControl);
        if let Some(original) = original {
            context
                .deadlines
                .arm(PumpDeadline::StateControl, original)
                .unwrap();
        }
        assert!(context.can_park_expired_startup_read(&permit));
    }
    if summary_evidence.is_some() {
        let context = &mut owner.run.context;
        for (origin, enter, held_leave) in [
            (Arc::new(()), original_enter, leave),
            (
                Arc::clone(&context.read_origin),
                AbsoluteBound::from_absolute(
                    original_enter.began_ns() + 1,
                    original_enter.deadline_ns(),
                )
                .unwrap(),
                leave,
            ),
            (
                Arc::clone(&context.read_origin),
                original_enter,
                AbsoluteBound::from_absolute(leave.began_ns() + 1, leave.deadline_ns()).unwrap(),
            ),
        ] {
            assert!(
                !context.can_detach_expired_boot_summary(&ExpiredStartupAccountPermit {
                    origin,
                    enter,
                    leave: held_leave
                })
            );
        }
        context.expired_summary_guard_probes(&ExpiredStartupAccountPermit {
            origin: Arc::clone(&context.read_origin),
            enter: original_enter,
            leave,
        });
    }
    if mode.starts_with("expired-startup-summary-account-") || summary_boundary::is_settled(mode) {
        assert_eq!(
            owner.advance_with_io(&mut CircuitIo(clock)),
            FoundationExpiredRollbackProgress::Accepted
        );
        assert!(
            owner.active.is_some(),
            "expired summary account must enter actual cleanup"
        );
    }
    if (mode.starts_with("expired-startup-summary-account-") || summary_boundary::is_settled(mode))
        && !mode.ends_with("-cap")
    {
        assert_eq!(
            owner.advance_with_io(&mut CircuitIo(clock)),
            FoundationExpiredRollbackProgress::Accepted
        );
        let context = &mut owner.run.context;
        assert!(context.deadlines.get(PumpDeadline::Enter).is_none());
        let ordinary = context
            .begin_inspection(InspectionKind::Grants, None, &mut CircuitIo(clock))
            .unwrap_err();
        assert!(!ordinary.retained && ordinary.failure.is_none());
        let account = owner.active.as_ref().unwrap().summary.as_ref().unwrap();
        for (origin, enter, held_leave) in [
            (Arc::new(()), original_enter, leave),
            (
                Arc::clone(&context.read_origin),
                AbsoluteBound::from_absolute(
                    original_enter.began_ns() + 1,
                    original_enter.deadline_ns(),
                )
                .unwrap(),
                leave,
            ),
            (
                Arc::clone(&context.read_origin),
                original_enter,
                AbsoluteBound::from_absolute(leave.began_ns() + 1, leave.deadline_ns()).unwrap(),
            ),
        ] {
            let refused = context
                .begin_expired_summary_grants(
                    account,
                    &ExpiredStartupGrantsPermit {
                        origin,
                        enter,
                        leave: held_leave,
                    },
                    &mut CircuitIo(clock),
                )
                .unwrap_err();
            assert!(!refused.retained && refused.failure.is_none());
            assert!(context.inspection.is_none());
            assert!(context.deadlines.get(PumpDeadline::StateControl).is_none());
        }
    }
    if mode.ends_with("-complete")
        || mode.contains("-release-")
        || mode.ends_with("-classifier-varied")
        || mode.ends_with("-classifier-unreadable")
        || mode.ends_with("-classifier-held")
        || mode.ends_with("-read-held")
        || mode.ends_with("-template-held")
        || mode.ends_with("-classifying-held")
    {
        // This cleanup waits for child exit with no socket descriptors. The
        // general circuit fake only supplies readiness for actual interests.
        struct CleanupIo<'a>(&'a mut FakePumpIo);
        impl PumpIo for CleanupIo<'_> {
            fn now(&mut self) -> std::io::Result<u64> {
                self.0.now()
            }
            fn wait(
                &mut self,
                interests: &[WaitInterest<'_>],
                bound: Option<u64>,
            ) -> std::io::Result<crate::flow::clock::WaitOutcome> {
                if interests.is_empty() {
                    self.0.wait(interests, bound)
                } else {
                    CircuitIo(self.0).wait(interests, bound)
                }
            }
        }
        let limit = Instant::now() + Duration::from_secs(4);
        let mut release_child = !(mode.ends_with("-classifier-held")
            || mode.ends_with("-read-held")
            || mode.ends_with("-template-held")
            || mode.ends_with("-classifying-held"));
        let mut release_spu = !mode.ends_with("-release-held");
        let unsent_open = mode == "expired-startup-open-unsent-complete";
        let incomplete_open = mode.starts_with("expired-startup-open-") && !unsent_open;
        // Every child is already forked. Drain the real writer concurrently:
        // close_after_drain is itself a blocking operation under its bound.
        let trace_reader = (pending_open.is_some()
            || pending_read_record.is_some()
            || template_record.is_some()
            || filing_record.is_some())
        .then(|| {
            use std::io::BufRead;
            let input = reader.try_clone().unwrap();
            input
                .set_read_timeout(Some(Duration::from_secs(8)))
                .unwrap();
            std::thread::spawn(move || {
                let mut rows = vec![];
                for line in std::io::BufReader::new(input).lines() {
                    let row: serde_json::Value = serde_json::from_str(&line.unwrap()).unwrap();
                    let terminal = row["kind"] == "unload";
                    rows.push(row);
                    if terminal {
                        return rows;
                    }
                }
                panic!("pressure fixture reached EOF before Unload");
            })
        });
        let invalid_release = [
            "-release-wrong",
            "-release-arm",
            "-release-position",
            "-release-early",
        ]
        .iter()
        .any(|suffix| mode.ends_with(suffix));
        loop {
            assert!(
                Instant::now() < limit,
                "classifier rollback: {:?}",
                owner.detail()
            );
            let progress = owner.advance_with_io(&mut CleanupIo(clock));
            if unsent_gate.is_some()
                && let Some(gate) = owner
                    .active
                    .as_ref()
                    .and_then(|active| active.gate.as_ref())
            {
                assert!(
                    gate.test_lower().is_none(),
                    "unraised gate must never construct a Lower"
                );
            }
            if mode.contains("-admission-unsent-failure-")
                && owner
                    .active
                    .as_ref()
                    .is_some_and(|active| active.admission.is_some())
            {
                struct BrokenOnce<'a> {
                    clock: &'a mut FakePumpIo,
                    broken: bool,
                }
                impl PumpIo for BrokenOnce<'_> {
                    fn now(&mut self) -> std::io::Result<u64> {
                        if !self.broken {
                            self.broken = true;
                            Err(std::io::Error::other("admission rollback clock failed"))
                        } else {
                            self.clock.now()
                        }
                    }
                    fn wait(
                        &mut self,
                        _: &[WaitInterest<'_>],
                        _: Option<u64>,
                    ) -> std::io::Result<crate::flow::clock::WaitOutcome> {
                        panic!("clock failure cannot wait");
                    }
                }
                assert_eq!(
                    owner.advance_with_io(&mut BrokenOnce {
                        clock,
                        broken: false
                    }),
                    FoundationExpiredRollbackProgress::Stopped
                );
                let first = owner.run.context.failure.clone().unwrap();
                assert!(matches!(
                    first,
                    RunFailure::Flow {
                        reason: FlowHaltReason::TraceFailed,
                        ..
                    }
                ));
                let mut exit = ExitGuard(
                    guard
                        .0
                        .take()
                        .unwrap()
                        .failed_exit()
                        .ok()
                        .expect("failed admission rollback"),
                );
                use super::super::rollback::failed::FoundationRollbackExitProgress as ExitProgress;
                assert_eq!(exit.0.process_exit(), None);
                loop {
                    assert!(Instant::now() < limit, "failed admission cleanup");
                    assert_eq!(exit.0.failure(), Some(&first));
                    assert!(exit.0.deadline().unwrap() <= leave.deadline_ns());
                    let account = exit
                        .0
                        .test_admission()
                        .expect("failed exit retains admission");
                    assert_eq!(account.enter, original_enter);
                    assert_eq!(
                        &account.admission.test_evidence(),
                        admission_evidence.as_ref().unwrap()
                    );
                    assert!(account.admission.transport_retired());
                    if exit.0.advance_with_io(&mut CleanupIo(clock)) == ExitProgress::Closed {
                        break;
                    }
                    if exit.0.progress() == ExitProgress::Pending {
                        exit.0.wait_with_io(&mut CleanupIo(clock));
                    }
                    std::thread::sleep(Duration::from_millis(1));
                }
                assert!(!exit.0.abandoned());
                assert_eq!(
                    exit.0.process_exit(),
                    Some(crate::FoundationProcessExit::Failure)
                );
                assert!(exit.0.test_spu_child().is_none());
                assert!(packet(&peer).is_none(), "failed admission grants no Left");
                assert!(!path.parent().unwrap().join("classifier-bootstrap").exists());
                reader.set_nonblocking(true).unwrap();
                let rows = super::super::tests::trace_through(
                    reader,
                    vec![],
                    weaver_trace::Sequence(exit.0.unload_sequence().unwrap()),
                    limit,
                )
                .unwrap();
                assert_eq!(rows.iter().filter(|row| row["kind"] == "unload").count(), 1);
                assert!(rows.iter().any(|row| row["kind"] == "fault"));
                assert!(!rows.iter().any(|row| row["kind"] == "ste100.result"));
                assert!(
                    rows.last().unwrap()["payload"]["release"]
                        .get("spu")
                        .is_none()
                );
                return;
            }
            let template_failure =
                mode.contains("-template-component-") || mode.contains("-template-foreign-");
            if template_failure && progress == FoundationExpiredRollbackProgress::Stopped {
                let expected_reason = if mode.contains("-component-") {
                    FlowHaltReason::ClassifierFailed
                } else {
                    FlowHaltReason::ProtocolFailed
                };
                assert!(
                    matches!(owner.run.context.failure.as_ref(), Some(RunFailure::Flow { reason, .. }) if *reason == expected_reason)
                );
                assert_eq!(
                    owner.run.context.flow.classifier().held(),
                    template_reply.as_ref()
                );
                assert!(
                    owner.active.as_ref().unwrap().templates.is_none(),
                    "failure retains original context custody"
                );
                assert!(owner.unload_sequence().is_none() && packet(&peer).is_none());
                let first = owner.run.context.failure.clone().unwrap();
                let mut exit = ExitGuard(
                    guard
                        .0
                        .take()
                        .unwrap()
                        .failed_exit()
                        .ok()
                        .expect("actual failed template rollback"),
                );
                use super::super::rollback::failed::FoundationRollbackExitProgress as ExitProgress;
                assert_eq!(exit.0.process_exit(), None);
                loop {
                    assert!(
                        Instant::now() < limit,
                        "failed template rollback did not close"
                    );
                    assert_eq!(exit.0.failure(), Some(&first));
                    assert!(exit.0.deadline().unwrap() <= leave.deadline_ns());
                    if exit.0.advance_with_io(&mut CleanupIo(clock)) == ExitProgress::Closed {
                        break;
                    }
                    if exit.0.progress() == ExitProgress::Pending {
                        exit.0.wait_with_io(&mut CleanupIo(clock));
                    }
                    std::thread::sleep(Duration::from_millis(1));
                }
                assert!(!exit.0.abandoned());
                assert_eq!(
                    exit.0.process_exit(),
                    Some(crate::FoundationProcessExit::Failure)
                );
                assert!(
                    packet(&peer).is_none(),
                    "failed template rollback grants no Left"
                );
                let sequence = exit
                    .0
                    .unload_sequence()
                    .expect("bounded failed template Unload");
                let rows = trace_reader.unwrap().join().unwrap();
                assert_eq!(rows.iter().filter(|row| row["kind"] == "unload").count(), 1);
                assert!(rows.iter().any(|row| row["kind"] == "fault"));
                if mode.contains("-template-component-") {
                    let results = rows
                        .iter()
                        .filter(|row| row["kind"] == "ste100.result")
                        .collect::<Vec<_>>();
                    assert_eq!(
                        results.len(),
                        1,
                        "the actual failure reply records once under the halt cap"
                    );
                    assert_eq!(
                        results[0]["payload"],
                        serde_json::to_value(template_reply.as_ref().unwrap()).unwrap()
                    );
                }
                assert_eq!(
                    rows.last().unwrap()["sequence"]
                        .as_str()
                        .unwrap()
                        .parse::<u64>()
                        .unwrap(),
                    sequence
                );
                assert_eq!(
                    rows.last().unwrap()["payload"]["end_reason"],
                    "component_failure"
                );
                return;
            }
            if invalid_release && progress == FoundationExpiredRollbackProgress::Stopped {
                assert!(owner.run.context.flow.halt().is_some());
                assert!(owner.unload_sequence().is_none());
                assert!(packet(&peer).is_none(), "invalid Release granted Left");
                let release = owner.active.as_ref().unwrap().release.as_ref().unwrap();
                let reply = release
                    .received_reply()
                    .expect("retain actual invalid reply");
                assert!(release.outcome().is_none());
                assert!(
                    matches!(
                        owner.run.context.failure.as_ref(),
                        Some(RunFailure::Flow {
                            reason: FlowHaltReason::ProtocolFailed,
                            ..
                        })
                    ),
                    "{:?}",
                    owner.run.context.failure
                );
                if mode.ends_with("-early") {
                    assert!(!release.release_sent(), "reply preceded every Release byte");
                    assert!(matches!(
                        reply.payload,
                        Payload::Answer(LifecycleAnswer::Released)
                    ));
                } else if mode.ends_with("-arm") {
                    assert!(matches!(
                        reply.payload,
                        Payload::Answer(LifecycleAnswer::Admitted)
                    ));
                } else if mode.ends_with("-position") {
                    assert_eq!(reply.position, Position::Open);
                } else {
                    assert_eq!(reply.exchange.ordinal, 3);
                }
                let first = owner.run.context.failure.clone().unwrap();
                let leave = owner.leave.unwrap();
                let cause = owner.original.as_ref().unwrap().payload().cause.uid;
                let mut exit = ExitGuard(
                    guard
                        .0
                        .take()
                        .unwrap()
                        .failed_exit()
                        .ok()
                        .expect("actual stopped rollback"),
                );
                use super::super::rollback::failed::FoundationRollbackExitProgress as ExitProgress;
                assert_eq!(exit.0.process_exit(), None);
                loop {
                    assert!(
                        Instant::now() < limit,
                        "failed expired rollback did not close"
                    );
                    assert_eq!(exit.0.failure(), Some(&first));
                    assert!(exit.0.deadline().unwrap() <= leave.deadline_ns());
                    if exit.0.advance_with_io(&mut CleanupIo(clock)) == ExitProgress::Closed {
                        break;
                    }
                    if exit.0.progress() == ExitProgress::Pending {
                        exit.0.wait_with_io(&mut CleanupIo(clock));
                    }
                    std::thread::sleep(Duration::from_millis(1));
                }
                assert!(!exit.0.abandoned());
                assert_eq!(
                    exit.0.process_exit(),
                    Some(crate::FoundationProcessExit::Failure)
                );
                if incomplete_open {
                    let account = exit
                        .0
                        .test_open()
                        .expect("failed exit retains Open account");
                    assert!(account.incomplete());
                    assert_eq!(account.request(), open_request.as_ref().unwrap());
                    assert_eq!(account.test_bound(), original_enter.deadline_ns());
                }
                if let Some(expected) = read_evidence.as_ref() {
                    let read = exit
                        .0
                        .test_read()
                        .expect("failed exit retains original read");
                    assert_eq!(
                        &(
                            read.plan_bound(),
                            read.plan_expected().cloned(),
                            read.acquired_snapshot(),
                            read.assembled_bytes().to_vec(),
                            read.plan_ticket(),
                            read.plan_recorded()
                        ),
                        expected
                    );
                }
                if let Some(expected) = boot_pair_evidence.as_ref() {
                    assert_eq!(
                        &exit
                            .0
                            .test_boot_pairs()
                            .expect("failed exit retains original boot repair")
                            .test_evidence(),
                        expected
                    );
                }
                if let Some(author) = filing_author.as_ref() {
                    assert_eq!(
                        exit.0.test_boot_pairs().unwrap().test_filing().as_ref(),
                        Some(author)
                    );
                    let context = exit.0.test_context().unwrap();
                    assert_eq!(
                        Some(context.flow.test_boot_filing_evidence()).as_ref(),
                        filing_flow.as_ref()
                    );
                    assert_eq!(
                        context.flow.test_boot_filing_channel().as_ref(),
                        filing_channel.as_ref()
                    );
                    assert!(context.flow.test_boot_filing_retired());
                    assert!(context.flow.test_boot_filing_progress_current());
                    assert!(context.filing.is_none());
                    member.assert_closed_without_request();
                }
                if let Some(expected) = summary_evidence.as_ref() {
                    assert_eq!(
                        &exit
                            .0
                            .test_summary()
                            .expect("failed exit retains summary account")
                            .expired_test_evidence(),
                        expected
                    );
                    let context = exit.0.test_context().unwrap();
                    assert!(context.plan_summary.is_none());
                    assert_eq!(
                        Some(context.expired_summary_cache_evidence()).as_ref(),
                        summary_cache.as_ref()
                    );
                }
                if template_target {
                    let account = exit
                        .0
                        .test_templates()
                        .expect("failed exit retains template account");
                    assert_eq!(account.check.request(), template_request.as_ref());
                    assert_eq!(account.check.reply(), template_reply.as_ref());
                    assert_eq!(account.enter(), original_enter);
                    assert!(account.check.transport_retired());
                }
                assert_eq!(exit.0.channel_fault(), Some(crate::ChannelFault::Closed));
                assert!(matches!(
                    exit.0.test_spu_child(),
                    Some(
                        crate::flow::context::failed_enter_driver::ChildState::Reaped(
                            nix::sys::wait::WaitStatus::Exited(_, 0)
                        )
                    )
                ));
                assert!(packet(&peer).is_none(), "failed exit cannot grant Left");
                let sequence = exit.0.unload_sequence().expect("bounded failure Unload");
                reader.set_nonblocking(true).unwrap();
                let rows = super::super::tests::trace_through(
                    reader,
                    vec![],
                    weaver_trace::Sequence(sequence),
                    limit,
                )
                .unwrap();
                assert_eq!(rows.iter().filter(|row| row["kind"] == "unload").count(), 1);
                assert!(rows.iter().any(|row| row["kind"] == "fault"));
                let terminal = rows.last().unwrap();
                assert_eq!(terminal["kind"], "unload");
                assert_eq!(terminal["payload"]["cause"]["uid"], cause);
                assert_eq!(terminal["payload"]["release"]["spu"], "unconfirmed");
                assert_eq!(terminal["payload"]["end_reason"], "component_failure");
                return;
            }
            assert_ne!(
                progress,
                FoundationExpiredRollbackProgress::Stopped,
                "{:?}",
                owner.detail()
            );
            if progress == FoundationExpiredRollbackProgress::Closed {
                assert!(
                    release_child && release_spu && !invalid_release && !template_failure,
                    "Left cannot precede the actual classifier reap"
                );
                break;
            }
            if mode.ends_with("-release-recorded") && owner.unload_sequence().is_some() {
                struct BrokenOnce<'a> {
                    clock: &'a mut FakePumpIo,
                    broken: bool,
                }
                impl PumpIo for BrokenOnce<'_> {
                    fn now(&mut self) -> std::io::Result<u64> {
                        if !self.broken {
                            self.broken = true;
                            Err(std::io::Error::other("post-Unload clock failed"))
                        } else {
                            self.clock.now()
                        }
                    }
                    fn wait(
                        &mut self,
                        _: &[WaitInterest<'_>],
                        _: Option<u64>,
                    ) -> std::io::Result<crate::flow::clock::WaitOutcome> {
                        panic!("post-Unload clock failure cannot wait")
                    }
                }
                let sequence = owner.unload_sequence().unwrap();
                assert_eq!(
                    owner.advance_with_io(&mut BrokenOnce {
                        clock,
                        broken: false
                    }),
                    FoundationExpiredRollbackProgress::Stopped
                );
                let mut exit = ExitGuard(
                    guard
                        .0
                        .take()
                        .unwrap()
                        .failed_exit()
                        .ok()
                        .expect("actual Recorded terminal"),
                );
                use super::super::rollback::failed::FoundationRollbackExitProgress as ExitProgress;
                assert_eq!(exit.0.process_exit(), None);
                loop {
                    assert!(Instant::now() < limit);
                    assert_eq!(exit.0.unload_sequence(), Some(sequence));
                    if exit.0.advance_with_io(&mut CleanupIo(clock)) == ExitProgress::Closed {
                        break;
                    }
                    exit.0.wait_with_io(&mut CleanupIo(clock));
                    std::thread::sleep(Duration::from_millis(1));
                }
                assert!(!exit.0.abandoned());
                assert_eq!(
                    exit.0.process_exit(),
                    Some(crate::FoundationProcessExit::Failure)
                );
                assert!(packet(&peer).is_none());
                reader.set_nonblocking(true).unwrap();
                let rows = super::super::tests::trace_through(
                    reader,
                    vec![],
                    weaver_trace::Sequence(sequence),
                    limit,
                )
                .unwrap();
                assert_eq!(rows.iter().filter(|row| row["kind"] == "unload").count(), 1);
                assert!(!rows.iter().any(|row| row["kind"] == "fault"));
                let terminal = rows.last().unwrap();
                assert_eq!(terminal["payload"]["release"]["spu"], "confirmed");
                assert!(
                    terminal["payload"]
                        .get("end_reason")
                        .is_none_or(serde_json::Value::is_null)
                );
                return;
            }
            if mode.ends_with("-release-silent")
                && path.parent().unwrap().join("release-received").exists()
            {
                let leave = owner.leave.unwrap();
                assert!(clock.now().unwrap() < leave.deadline_ns());
                // No ready descriptor: the actual release wait must consume the
                // original Leave, then retain and abandon without a terminal.
                assert_eq!(
                    owner.advance_with_io(clock),
                    FoundationExpiredRollbackProgress::Closed
                );
                assert_eq!(clock.now().unwrap(), leave.deadline_ns());
                assert!(owner.abandoned());
                assert_eq!(
                    owner.process_exit(),
                    Some(crate::FoundationProcessExit::Failure)
                );
                assert!(owner.run.context.failure.is_none());
                assert!(owner.unload_sequence().is_none());
                assert!(packet(&peer).is_none());
                assert!(
                    owner
                        .active
                        .as_ref()
                        .unwrap()
                        .release
                        .as_ref()
                        .unwrap()
                        .outcome()
                        .is_none()
                );
                return;
            }
            if !release_spu && owner.unload_sequence().is_some() {
                assert!(path.parent().unwrap().join("release-replied").exists());
                assert!(matches!(
                    owner.run.resources.spu_child(),
                    Some(crate::flow::context::failed_enter_driver::ChildState::Running)
                ));
                for _ in 0..4 {
                    assert_eq!(
                        owner.advance_with_io(&mut CleanupIo(clock)),
                        FoundationExpiredRollbackProgress::Accepted
                    );
                    assert!(packet(&peer).is_none(), "Left requires actual SPU reap");
                }
                std::fs::write(path.parent().unwrap().join("spu-exit"), b"exit").unwrap();
                release_spu = true;
            }
            if !release_child && path.parent().unwrap().join("classifier-retired").exists() {
                assert!(
                    !path.exists(),
                    "the lifecycle seal must precede classifier release"
                );
                assert!(matches!(
                    owner.run.resources.classifier_child(),
                    Some(crate::flow::context::failed_enter_driver::ChildState::Running)
                ));
                for _ in 0..4 {
                    assert_eq!(
                        owner.advance_with_io(&mut CleanupIo(clock)),
                        FoundationExpiredRollbackProgress::Accepted
                    );
                    assert!(packet(&peer).is_none(), "no Left before the actual reap");
                    assert!(
                        owner.unload_sequence().is_none(),
                        "healthy Unload requires classifier reap first"
                    );
                    member.assert_no_request();
                    if filing_pending {
                        assert_eq!(
                            owner.run.context.flow.test_boot_filing_channel().as_ref(),
                            filing_channel.as_ref()
                        );
                        assert!(
                            !owner.run.context.flow.test_boot_filing_retired(),
                            "member stays live until the actual classifier reap"
                        );
                    } else if !pending_read {
                        assert!(
                            owner.run.context.flow.standing_state().is_some(),
                            "healthy member release follows classifier reap"
                        );
                    }
                }
                let root = path.parent().unwrap();
                std::fs::write(root.join("classifier-exit"), b"exit").unwrap();
                release_child = true;
            }
            member.step(
                clock.now().unwrap(),
                if mode.ends_with("-unreadable") {
                    "rollback-grants-refused"
                } else if mode.ends_with("-varied") {
                    "rollback-varied"
                } else {
                    "rollback-ready"
                },
            );
            assert!(packet(&peer).is_none(), "Left preceded completed rollback");
            std::thread::sleep(Duration::from_millis(1));
        }
        assert!(!owner.abandoned());
        assert!(owner.run.context.failure.is_none() && owner.run.context.flow.halt().is_none());
        if let Some((raise, outgoing)) = &unsent_gate {
            let HeldStartup::Gate(held) = owner.handoff.held.as_ref() else {
                panic!("original unraised gate remains owned");
            };
            assert_eq!(&serde_json::to_vec(&held.raise).unwrap(), raise);
            assert!(
                held.send
                    .as_ref()
                    .is_none_or(|send| !send.sent() && send.sent_at().is_none()),
                "rollback never flushes the original Raise"
            );
            assert!(held.reply.is_none() && held.returned.is_none());
            let gate = owner.active.as_ref().unwrap().gate.as_ref().unwrap();
            assert!(
                gate.test_lower().is_none(),
                "an unsent Raise cannot produce Lower"
            );
            assert!(gate.test_replies().0.is_none() && gate.test_replies().1.is_none());
            let retained = gate
                .test_retired_outgoing()
                .map(|(bytes, offset)| (bytes.to_vec(), bytes.as_ptr() as usize, offset));
            assert_eq!(
                &retained, outgoing,
                "retirement preserves the exact original outgoing buffer and offset"
            );
            let arms = owner.handoff.gate.as_ref().unwrap();
            assert!(arms.port.is_none());
            if outgoing.is_some() {
                assert!(gate.test_retired());
                assert!(matches!(
                    arms.child.as_ref().unwrap().state(),
                    crate::flow::context::failed_enter_driver::ChildState::Reaped(_)
                ));
            } else {
                assert!(arms.child.is_none() && !gate.test_retired());
            }
        }
        assert_eq!(
            owner.run.context.retired_enter,
            Some(owner.handoff.identity.enter)
        );
        if classifier_started {
            assert!(matches!(
                owner.run.resources.classifier_child(),
                Some(crate::flow::context::failed_enter_driver::ChildState::Reaped(_))
            ));
        } else {
            assert!(owner.run.resources.classifier_child().is_none());
        }
        let spu = spu_started;
        if spu {
            assert!(matches!(
                owner.run.resources.spu_child(),
                Some(
                    crate::flow::context::failed_enter_driver::ChildState::Reaped(
                        nix::sys::wait::WaitStatus::Exited(_, 0)
                    )
                )
            ));
        } else {
            assert!(owner.run.resources.spu_child().is_none());
        }
        assert!(owner.run.context.flow.standing_state().is_none());
        assert_eq!(
            owner.active.as_ref().unwrap().current.is_none(),
            pending_read || filing_pending
        );
        if mode == "expired-startup-admit-sent-complete" {
            let release = owner.active.as_ref().unwrap().release.as_ref().unwrap();
            let prior = release
                .prior_reply()
                .expect("retain actual late Admit reply");
            assert_eq!(prior.exchange.ordinal, 1);
            assert!(matches!(
                prior.payload,
                Payload::Answer(LifecycleAnswer::Admitted)
            ));
            assert_eq!(release.received_reply().unwrap().exchange.ordinal, 2);
        }
        let mut expected_requests = requests.clone();
        if !pending_read && !filing_pending {
            expected_requests.push("grants".into());
        }
        assert_eq!(member.requests, expected_requests);
        member.assert_closed_without_request();
        if let Some(expected) = summary_evidence.as_ref() {
            assert_eq!(
                &owner
                    .active
                    .as_ref()
                    .unwrap()
                    .summary
                    .as_ref()
                    .expect("retain exact summary account")
                    .expired_test_evidence(),
                expected
            );
            if mode.contains("-refused-") {
                let account = owner.active.as_ref().unwrap().summary.as_ref().unwrap();
                assert_eq!(
                    account.refusal(),
                    Some(&crate::flow::read::ReadFailure::Refused {
                        code: s::ReadRefusalCode::ResultTooLarge,
                        detail: "original boot summary refusal".into(),
                    })
                );
                assert!(
                    account.recorded_read().is_none(),
                    "refused summary is not a recorded null"
                );
            }
            assert!(owner.run.context.plan_summary.is_none());
            assert_eq!(
                Some(owner.run.context.expired_summary_cache_evidence()).as_ref(),
                summary_cache.as_ref()
            );
            assert!(matches!(
                owner.run.context.committed_plan_summary(),
                Err(crate::flow::plan::PlanSummaryUnavailable::Revoked)
            ));
        }
        if let Some(expected) = summary_boundary_evidence.as_ref() {
            summary_boundary::assert_cleanup(owner, expected, mode);
        }
        if let Some(expected) = extracted_summary.as_ref() {
            let HeldStartup::Boot(boot) = owner.handoff.held.as_ref() else {
                panic!("original boot summary handoff");
            };
            assert_eq!(&format!("{:?}", boot.summary.as_ref().unwrap()), expected);
        }
        if let Some(expected) = boot_pair_evidence.as_ref() {
            let account = boot_pair_account(owner).expect("retained original boot repair");
            assert_eq!(&account.test_evidence(), expected);
            assert!(owner.run.context.boot_pairs.is_none());
            if let Some(author) = filing_author.as_ref() {
                assert_eq!(account.test_filing().as_ref(), Some(author));
                assert!(owner.run.context.filing.is_none());
                assert_eq!(
                    owner.run.context.flow.test_boot_filing_record(),
                    filing_record
                );
                assert_eq!(
                    Some(owner.run.context.flow.test_boot_filing_evidence()).as_ref(),
                    filing_flow.as_ref()
                );
                assert_eq!(
                    Some(format!(
                        "{:?}",
                        (&owner.run.context.inbox, &owner.run.context.budget)
                    ))
                    .as_ref(),
                    filing_intake.as_ref()
                );
                if let Some(channel) = filing_channel.as_ref() {
                    assert_eq!(
                        owner.run.context.flow.test_boot_filing_channel().as_ref(),
                        Some(channel)
                    );
                    assert!(owner.run.context.flow.test_boot_filing_retired());
                    assert!(owner.run.context.flow.test_boot_filing_progress_current());
                }
            }
            let actual: Vec<_> = member
                .filings
                .iter()
                .filter_map(|file| match file {
                    s::StateFrame::FilePair { filing, .. } => Some(filing.clone()),
                    _ => None,
                })
                .collect();
            assert_eq!(
                serde_json::to_vec(&actual).unwrap(),
                serde_json::to_vec(boot_pair_filings.as_ref().unwrap()).unwrap(),
                "rollback cannot start or repeat boot-pair filings"
            );
        }
        if initial_admission {
            assert!(owner.run.context.flow.starting_classifier().is_none());
            assert!(owner.run.context.flow.standing_classifier().is_none());
            if let Some(expected) = admission_evidence.as_ref() {
                let account = owner
                    .active
                    .as_ref()
                    .unwrap()
                    .admission
                    .as_ref()
                    .expect("retained admission account");
                assert_eq!(account.enter, original_enter);
                assert_eq!(
                    account.admission.completion(),
                    admission_completion.as_ref()
                );
                assert_eq!(&account.admission.test_evidence(), expected);
                assert!(account.admission.transport_retired());
            } else {
                assert!(owner.active.as_ref().unwrap().admission.is_none());
            }
            if mode.contains("-unsent-") || mode.contains("-before-fork-") {
                assert!(
                    !path.parent().unwrap().join("classifier-bootstrap").exists(),
                    "expired admission sent bootstrap bytes"
                );
            }
        }
        if template_target {
            let account = owner
                .active
                .as_ref()
                .unwrap()
                .templates
                .as_ref()
                .expect("original template account");
            assert_eq!(account.check.request(), template_request.as_ref());
            assert_eq!(account.check.reply(), template_reply.as_ref());
            assert_eq!(account.enter(), original_enter);
            assert!(account.check.transport_retired());
            let retained = account
                .transcript()
                .records()
                .iter()
                .map(|record| {
                    (
                        record.operation().clone(),
                        record.reply().clone(),
                        record.result_source().clone(),
                    )
                })
                .collect::<Vec<_>>();
            assert_eq!(&retained, template_records.as_ref().unwrap());
            if let Some((event, ticket)) = template_record.as_ref() {
                let (retained, retained_ticket) = account
                    .check
                    .test_record()
                    .expect("pending template result retained");
                assert_eq!(&serde_json::to_vec(retained).unwrap(), event);
                assert_eq!(&retained_ticket, ticket);
            }
            if mode.contains("-unsent-") {
                assert!(
                    !path.parent().unwrap().join("template-request").exists(),
                    "expired unsent request touched the wire"
                );
            }
        }
        if let Some(expected) = read_evidence.as_ref() {
            let read = owner
                .active
                .as_ref()
                .unwrap()
                .read
                .as_ref()
                .expect("retain expired read")
                .evidence();
            assert_eq!(
                &(
                    read.plan_bound(),
                    read.plan_expected().cloned(),
                    read.acquired_snapshot(),
                    read.assembled_bytes().to_vec(),
                    read.plan_ticket(),
                    read.plan_recorded()
                ),
                expected,
                "rollback retains exact original read custody"
            );
            assert_eq!(
                read.plan_bound().deadline_ns(),
                original_enter.deadline_ns()
            );
            assert!(owner.run.context.read.is_none());
            assert!(owner.run.context.plan_summary.is_none());
        }
        if let Some((expected, ticket)) = pending_read_record.as_ref() {
            let (event, retained_ticket) = owner
                .run
                .context
                .flow
                .test_parked_read_record()
                .expect("retained original read record");
            assert_eq!(&serde_json::to_vec(event).unwrap(), expected);
            assert_eq!(&retained_ticket, ticket);
        }
        if matches!(
            admission_completion,
            Some(crate::flow::ste100_socket::AdmissionPoll::Refused(_))
        ) {
            assert!(
                packet(&peer).is_none(),
                "unrecorded original refusal cannot grant Left"
            );
            assert_eq!(
                owner.process_exit(),
                Some(crate::FoundationProcessExit::Failure)
            );
            assert!(owner.retired_dial.as_ref().unwrap()._send.is_none());
            assert!(
                !leave.expired(clock.now().unwrap()),
                "no new refusal accounting window"
            );
        } else if filing_pending {
            assert!(!boot_pair_account(owner).unwrap().account_complete());
            assert_eq!(
                owner.process_exit(),
                Some(crate::FoundationProcessExit::Failure)
            );
            assert!(
                packet(&peer).is_none(),
                "unfinished filing cannot grant Left"
            );
            assert!(owner.retired_dial.as_ref().unwrap()._send.is_none());
        } else if incomplete_open {
            assert_eq!(
                owner.process_exit(),
                Some(crate::FoundationProcessExit::Failure)
            );
            let account = owner.active.as_ref().unwrap().open.as_ref().unwrap();
            assert!(
                account.incomplete(),
                "unanswered Open has no successful account"
            );
            assert_eq!(account.request(), open_request.as_ref().unwrap());
            assert_eq!(
                account.test_bound(),
                owner.handoff.identity.enter.deadline_ns()
            );
            if let Some((event, ticket)) = pending_open.as_ref() {
                let (retained, retained_ticket) = account
                    .test_record()
                    .expect("retain exact pending placement");
                assert_eq!(&serde_json::to_vec(retained).unwrap(), event);
                assert_eq!(&retained_ticket, ticket);
                assert_eq!(account.test_raw(), open_raw.as_deref());
            } else {
                assert!(path.parent().unwrap().join("open-unanswered").exists());
            }
            assert!(packet(&peer).is_none(), "incomplete Open cannot grant Left");
            assert!(owner.retired_dial.as_ref().unwrap()._send.is_none());
            assert!(
                !leave.expired(clock.now().unwrap()),
                "cleanup did not need the Leave cap"
            );
        } else {
            if unsent_open {
                let account = owner.active.as_ref().unwrap().open.as_ref().unwrap();
                assert!(
                    !account.incomplete(),
                    "unsent cancellation needs no Open answer"
                );
                assert_eq!(account.request(), open_request.as_ref().unwrap());
                assert_eq!(account.test_bound(), original_enter.deadline_ns());
                assert!(account.test_raw().is_none());
                assert!(account.test_record().is_none());
            }
            let answer = packet(&peer).expect("actual Left after the terminal drain");
            assert_eq!(
                owner.process_exit(),
                Some(crate::FoundationProcessExit::Success)
            );
            assert_eq!(answer.exchange, directive.exchange);
            assert!(matches!(
                answer.payload,
                Payload::Answer(LifecycleAnswer::Left {
                    save_point: None,
                    forced: true,
                    no_state: false
                })
            ));
        }
        let sequence = owner
            .unload_sequence()
            .expect("retained real Unload sequence");
        let rows = if let Some(trace_reader) = trace_reader {
            trace_reader.join().unwrap()
        } else {
            reader.set_nonblocking(true).unwrap();
            super::super::tests::trace_through(
                reader,
                vec![],
                weaver_trace::Sequence(sequence),
                limit,
            )
            .unwrap()
        };
        let terminal = rows.last().unwrap();
        if let Some(expected) = summary_boundary_evidence.as_ref() {
            summary_boundary::assert_trace(&rows, expected);
        }
        if summary_evidence.is_some() || extracted_summary.is_some() {
            let expected =
                usize::from(!mode.contains("-unstarted-") && !mode.contains("-refused-"));
            assert_eq!(
                rows.iter()
                    .filter(|row| row["kind"] == "recall"
                        && row["payload"]["read"]["selector"]
                            .get("plan_current")
                            .is_some())
                    .count(),
                expected,
                "rollback neither invents nor repeats summary recall"
            );
        }
        if let Some(files) = boot_pair_filings.as_ref() {
            let terminals: Vec<_> = rows
                .iter()
                .filter(|row| row["kind"] == "tool.call.completed")
                .collect();
            assert_eq!(
                terminals.len(),
                files.len().max(usize::from(filing_trace.is_some()))
            );
            if let Some((sequence, ended)) = filing_trace.as_ref() {
                let sequence_text = sequence.0.to_string();
                let terminal = terminals
                    .iter()
                    .find(|row| row["sequence"].as_str() == Some(sequence_text.as_str()))
                    .unwrap();
                assert_eq!(
                    terminal["payload"],
                    serde_json::to_value(crate::flow::mirror::pair_ended(ended)).unwrap()
                );
            }
            for (row, filing) in terminals.iter().zip(files) {
                let s::PairRecord::Ended(ended) = &filing.pair else {
                    unreachable!()
                };
                assert_eq!(row["sequence"], filing.source.sequence.0.to_string());
                assert_eq!(row["run"], filing.source.run);
                assert!(row["turn"].is_null());
                assert_eq!(
                    row["payload"],
                    serde_json::to_value(crate::flow::mirror::pair_ended(ended)).unwrap()
                );
            }
        }
        if let Some(records) = template_records.as_ref() {
            assert_eq!(
                rows.iter()
                    .filter(|row| row["kind"] == "ste100.result")
                    .count(),
                records.len() + template_extra_recorded,
                "expired check cannot author a fresh result or repeat a prior one"
            );
            if template_extra_recorded != 0 {
                let account = owner.active.as_ref().unwrap().templates.as_ref().unwrap();
                let source = account
                    .check
                    .test_recorded_source()
                    .expect("Recorded proof retained");
                let row = rows
                    .iter()
                    .rev()
                    .find(|row| row["kind"] == "ste100.result")
                    .unwrap();
                assert_eq!(row["sequence"], source.block_id.sequence.0.to_string());
                assert_eq!(row["run"], source.block_id.run);
                assert_eq!(
                    row["payload"],
                    serde_json::to_value(account.check.reply().unwrap()).unwrap()
                );
            }
        }
        assert_eq!(terminal["kind"], "unload");
        assert_eq!(
            terminal["payload"]["cause"]["uid"],
            owner.original.as_ref().unwrap().payload().cause.uid
        );
        assert_eq!(
            terminal["payload"]["grant_surface"],
            if pending_read || filing_pending || mode.ends_with("-unreadable") {
                "unreadable"
            } else if mode.ends_with("-varied") {
                "varied"
            } else {
                "unchanged"
            }
        );
        assert_eq!(terminal["payload"]["release"]["member"], "closed");
        if spu {
            assert_eq!(
                terminal["payload"]["release"]["spu"],
                if mode == "expired-startup-admit-complete"
                    || mode == "expired-startup-admit-pending-complete"
                {
                    "unconfirmed"
                } else {
                    "confirmed"
                }
            );
        } else {
            assert!(terminal["payload"]["release"].get("spu").is_none());
        }
        assert!(rows.iter().all(|row| row["kind"] != "fault"
            && (row["kind"] != "refusal" || row["payload"].get("fixture").is_some())));
        if incomplete_open || unsent_open {
            assert!(rows.iter().all(|row| row["kind"] != "state.placement"));
        }
        assert!(!path.exists());
        assert_eq!(
            owner.advance_with_io(clock),
            FoundationExpiredRollbackProgress::Closed
        );
        assert!(packet(&peer).is_none(), "no duplicate Left");
        return;
    }
    if mode.ends_with("-activation-trace") {
        let limit = Instant::now() + Duration::from_secs(3);
        while owner.unload_sequence().is_none() {
            assert!(
                Instant::now() < limit,
                "preflight terminal did not reach its actual trace admission"
            );
            assert_eq!(
                owner.advance_with_io(&mut CircuitIo(clock)),
                FoundationExpiredRollbackProgress::Accepted,
                "{:?}",
                owner.detail()
            );
            assert!(packet(&peer).is_none());
        }
        assert!(matches!(
            owner
                .run
                .context
                .barrier
                .drain_status(&mut owner.run.context.budget)
                .unwrap(),
            weaver_trace::serving::DrainStatus::Pending { .. }
        ));
        assert_eq!(
            owner.process_exit(),
            None,
            "Recorded Unload does not finish drain"
        );
        struct ExpireOnWait<'a>(&'a mut FakePumpIo, u64);
        impl PumpIo for ExpireOnWait<'_> {
            fn now(&mut self) -> std::io::Result<u64> {
                self.0.now()
            }
            fn wait(
                &mut self,
                _: &[WaitInterest<'_>],
                bound: Option<u64>,
            ) -> std::io::Result<crate::flow::clock::WaitOutcome> {
                assert_eq!(bound, Some(self.1), "drain retains the original Leave cap");
                self.0.advance_to(self.1).unwrap();
                Ok(crate::flow::clock::WaitOutcome::Timeout)
            }
        }
        assert_eq!(
            owner.advance_with_io(&mut ExpireOnWait(clock, leave.deadline_ns())),
            FoundationExpiredRollbackProgress::Closed,
            "{:?}",
            owner.detail()
        );
        assert!(owner.abandoned());
        assert_eq!(
            owner.process_exit(),
            Some(crate::FoundationProcessExit::Failure)
        );
        assert_eq!(
            owner.unload_sequence(),
            Some(1),
            "the original Recorded terminal stays evidence"
        );
        assert!(owner.run.context.failure().is_none());
        assert!(owner.run.context.flow.halt().is_none());
        assert!(owner.retired_dial.as_ref().unwrap()._send.is_none());
        assert!(packet(&peer).is_none(), "no Left from a blocked drain");
        assert!(!path.exists());
        assert_eq!(member.requests, requests);
        // Permit the abandoned asynchronous writer to leave its in-flight
        // syscall. A queued receipt need not survive a forced process exit.
        let _ = reader.shutdown(std::net::Shutdown::Both);
        return;
    }
    if mode.ends_with("-activation") {
        let limit = Instant::now() + Duration::from_secs(3);
        loop {
            assert!(
                Instant::now() < limit,
                "preflight rollback did not complete: {:?}",
                owner.detail()
            );
            let progress = owner.advance_with_io(&mut CircuitIo(clock));
            assert_ne!(
                progress,
                FoundationExpiredRollbackProgress::Stopped,
                "{:?}",
                owner.detail()
            );
            if progress == FoundationExpiredRollbackProgress::Closed {
                break;
            }
            assert!(packet(&peer).is_none(), "Left precedes completed rollback");
            std::thread::sleep(Duration::from_millis(1));
        }
        assert!(!owner.abandoned());
        assert_eq!(owner.run.context.phase, RecordingPhase::Closed);
        assert_eq!(
            owner.process_exit(),
            Some(crate::FoundationProcessExit::Success)
        );
        assert!(owner.run.context.failure().is_none());
        assert!(owner.run.context.flow.halt().is_none());
        assert!(owner.run.context.flow.unused_open().is_some());
        assert!(owner.run.context.flow.standing_state().is_none());
        assert_eq!(
            owner.run.context.retired_enter,
            Some(owner.handoff.identity.enter)
        );
        assert_eq!(
            owner.run.context.deadlines.get(PumpDeadline::Leave),
            Some(leave)
        );
        assert_eq!(owner.unload_sequence(), Some(1));
        let answer = packet(&peer).expect("actual Left follows Unload and drain");
        assert_eq!(answer.exchange, directive.exchange);
        assert!(matches!(
            answer.payload,
            Payload::Answer(LifecycleAnswer::Left {
                save_point: None,
                forced: true,
                no_state: false
            })
        ));
        assert_eq!(member.requests, requests);
        member.assert_closed_without_request();
        reader.set_nonblocking(true).unwrap();
        let rows =
            super::super::tests::trace_through(reader, vec![], weaver_trace::Sequence(1), limit)
                .unwrap();
        assert_eq!(
            rows.iter()
                .map(|row| row["kind"].as_str().unwrap())
                .collect::<Vec<_>>(),
            ["load", "unload"]
        );
        let unload = &rows[1]["payload"];
        assert_eq!(
            unload["cause"]["uid"],
            owner.original.as_ref().unwrap().payload().cause.uid
        );
        assert_eq!(unload["forced"], true);
        assert_eq!(unload["grant_surface"], "unreadable");
        assert!(unload["release"].get("spu").is_none());
        assert_eq!(unload["release"]["member"], "closed");
        assert!(!path.exists());
        assert_eq!(
            owner.advance_with_io(clock),
            FoundationExpiredRollbackProgress::Closed
        );
        assert!(packet(&peer).is_none(), "no repeated Left");
        assert_eq!(
            owner.original.as_ref().unwrap().retained_wire().as_ptr(),
            address
        );
        completed_exit_guards(owner);
        return;
    }
    assert!(!owner.abandoned());
    assert!(owner.channel_fault().is_none());
    assert_eq!(owner.process_exit(), None);
    if initial_admission {
        assert_eq!(
            owner.advance_with_io(&mut CircuitIo(clock)),
            FoundationExpiredRollbackProgress::Accepted
        );
        let account = owner.active.as_ref().unwrap().admission.as_ref().unwrap();
        assert!(!account.admission.transport_retired());
        assert_eq!(
            &account.admission.test_evidence(),
            admission_evidence.as_ref().unwrap()
        );
    }
    if mode == "expired-startup-template-cap" {
        assert_eq!(
            owner.advance_with_io(&mut CircuitIo(clock)),
            FoundationExpiredRollbackProgress::Accepted
        );
        let account = owner
            .active
            .as_ref()
            .unwrap()
            .templates
            .as_ref()
            .expect("template account moved before cap");
        assert!(!account.check.transport_retired());
    }
    let had_gate_port = owner
        .handoff
        .gate
        .as_ref()
        .is_some_and(|gate| gate.port.is_some());
    clock.advance_to(leave.deadline_ns() - 1).unwrap();
    assert_eq!(
        owner.advance_with_io(clock),
        FoundationExpiredRollbackProgress::Accepted
    );
    assert!(
        owner.retired.is_none(),
        "the cap is not early cleanup authority"
    );
    clock.advance_to(leave.deadline_ns()).unwrap();
    assert_eq!(
        owner.advance_with_io(clock),
        FoundationExpiredRollbackProgress::Closed
    );
    assert!(owner.abandoned());
    assert_eq!(owner.channel_fault(), Some(crate::ChannelFault::Closed));
    assert_eq!(
        owner.process_exit(),
        Some(crate::FoundationProcessExit::Failure)
    );
    assert!(owner.run.context.failure().is_none());
    assert!(owner.run.context.flow.halt().is_none());
    assert_eq!(owner.run.context.phase, RecordingPhase::Closed);
    assert!(owner.run.context.flow.standing_state().is_none());
    assert!(
        owner.retired.as_ref().unwrap().state.is_some()
            || owner.retired.as_ref().unwrap().has_stopped_plan_read()
            || owner.run.context.flow.unused_open().is_some()
            || owner.run.context.flow.test_boot_filing_retired()
    );
    if let Some(expected) = boot_pair_evidence.as_ref() {
        assert_eq!(
            &boot_pair_account(owner)
                .expect("cap retains boot repair")
                .test_evidence(),
            expected
        );
    }
    if let Some(author) = filing_author.as_ref() {
        assert_eq!(
            boot_pair_account(owner).unwrap().test_filing().as_ref(),
            Some(author)
        );
        assert_eq!(
            Some(owner.run.context.flow.test_boot_filing_evidence()).as_ref(),
            filing_flow.as_ref()
        );
        assert_eq!(
            owner.run.context.flow.test_boot_filing_channel().as_ref(),
            filing_channel.as_ref()
        );
        assert!(owner.run.context.flow.test_boot_filing_retired());
        assert!(owner.run.context.flow.test_boot_filing_progress_current());
        assert_eq!(
            Some(format!(
                "{:?}",
                (&owner.run.context.inbox, &owner.run.context.budget)
            ))
            .as_ref(),
            filing_intake.as_ref()
        );
        assert!(
            owner.unload_sequence().is_none(),
            "cap grants no new Unload"
        );
        member.assert_closed_without_request();
    }
    if let Some(expected) = summary_evidence.as_ref() {
        assert_eq!(
            &owner
                .active
                .as_ref()
                .unwrap()
                .summary
                .as_ref()
                .expect("cap retains exact summary account")
                .expired_test_evidence(),
            expected
        );
        assert!(owner.run.context.plan_summary.is_none());
        assert_eq!(
            Some(owner.run.context.expired_summary_cache_evidence()).as_ref(),
            summary_cache.as_ref()
        );
    }
    assert!(owner.run.resources.decode_present_for_serving().is_none());
    assert!(owner.run.resources.residency_port().is_none());
    if initial_admission {
        let account = owner.active.as_ref().unwrap().admission.as_ref().unwrap();
        assert!(
            account.admission.transport_retired(),
            "cap closes admission descriptor"
        );
        assert_eq!(
            &account.admission.test_evidence(),
            admission_evidence.as_ref().unwrap()
        );
        assert_eq!(account.enter, original_enter);
    }
    if mode == "expired-startup-template-cap" {
        let account = owner.active.as_ref().unwrap().templates.as_ref().unwrap();
        assert!(
            account.check.transport_retired(),
            "cap closes actual retained classifier descriptor"
        );
        assert_eq!(account.check.request(), template_request.as_ref());
        assert_eq!(account.enter(), original_enter);
    }
    assert!(!path.exists());
    assert!(owner.handoff.listener.is_none());
    assert!(
        owner.retired_listener.is_some() || owner.preflight.is_some() || owner.active.is_some()
    );
    let retained = owner.retired_dial.as_ref().unwrap();
    assert_eq!(retained._message.as_ref().unwrap().envelope, directive);
    assert!(
        retained._send.is_none(),
        "no fabricated Left at the cleanup cap"
    );
    assert_eq!(owner.original.as_ref().unwrap().retained_wire(), original);
    assert_eq!(
        owner.original.as_ref().unwrap().retained_wire().as_ptr(),
        address
    );
    assert!(owner.run.context.boot_open_account_settled());
    if mode.ends_with("-open") {
        assert!(
            owner.run.context.boot_open_incomplete().is_some()
                || owner
                    .active
                    .as_ref()
                    .and_then(|active| active.open.as_ref())
                    .is_some_and(|open| open.incomplete())
        );
    }
    if let Some(gate) = &owner.handoff.gate {
        assert!(gate.port.is_none());
    }
    assert_eq!(owner.retired_gate.is_some(), had_gate_port);
    assert_eq!(
        owner.advance_with_io(clock),
        FoundationExpiredRollbackProgress::Closed
    );
    assert_eq!(
        member.requests, requests,
        "disposal does not resume expired boot work"
    );
}

/// Vary each disqualifier on the real completed preflight owner while retaining
/// its successful inner terminal. These are disposition checks, not claims that
/// an already closed cleanup admits new runtime work.
fn completed_exit_guards(owner: &mut FoundationExpiredRollback) {
    use crate::FoundationProcessExit::{Failure, Success};
    assert_eq!(owner.progress(), FoundationExpiredRollbackProgress::Closed);
    assert_eq!(owner.process_exit(), Some(Success));
    owner
        .preflight
        .as_mut()
        .unwrap()
        .test_completed_exit_guards();
    let sequence = owner.unload_sequence();

    owner.abandoned = true;
    assert_eq!(
        owner.process_exit(),
        Some(Failure),
        "abandonment overrides a successful preflight terminal"
    );
    owner.abandoned = false;
    assert_eq!(owner.process_exit(), Some(Success));

    owner.run.context.failure = Some(RunFailure::Flow {
        reason: FlowHaltReason::TraceFailed,
        point: HaltPoint::Teardown,
        detail: "retained completion failure".into(),
    });
    assert_eq!(
        owner.process_exit(),
        Some(Failure),
        "retained failure overrides a successful preflight terminal"
    );
    owner.run.context.failure = None;
    assert_eq!(owner.process_exit(), Some(Success));

    let context = &mut owner.run.context;
    context.flow.latch(
        crate::flow::report::SecondaryFailure {
            reason: FlowHaltReason::ProtocolFailed,
            point: HaltPoint::Teardown,
            detail: "retained completion halt".into(),
            observed_ns: Some(context.last_ns.into()),
        },
        context
            .deadlines
            .get(PumpDeadline::Leave)
            .map(AbsoluteBound::deadline_ns),
        &mut FlowRecording {
            barrier: &mut context.barrier,
            inbox: &mut context.inbox,
            budget: &mut context.budget,
            author: &context.author,
        },
    );
    assert!(
        context.failure.is_none(),
        "the halt guard is exercised independently"
    );
    assert!(context.flow.halt().is_some());
    assert_eq!(
        owner.preflight.as_ref().unwrap().process_exit(),
        Some(Success)
    );
    assert_eq!(
        owner.process_exit(),
        Some(Failure),
        "retained halt overrides a successful preflight terminal"
    );
    assert_eq!(
        owner.unload_sequence(),
        sequence,
        "disposition grants no new author"
    );
}
