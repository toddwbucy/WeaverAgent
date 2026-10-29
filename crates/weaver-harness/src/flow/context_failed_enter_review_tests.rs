//! #280 actual owner/process followups. Helper-only defenses are labelled.
use super::*;
use std::os::fd::AsRawFd;
use std::time::Instant;

pub(super) fn await_decoder_eof(fd: i32, bound: Duration) {
    crate::flow::fixture_eof::await_eof_until(fd, Instant::now() + bound, || {});
}

#[test]
fn failed_enter_review_eof_wait_observes_real_close_and_never_accepts_eagain() {
    let (near, far) = UnixStream::pair().unwrap();
    let thread = std::thread::spawn(move || {
        std::thread::sleep(Duration::from_millis(10));
        drop(far);
    });
    await_decoder_eof(near.as_raw_fd(), Duration::from_secs(3));
    thread.join().unwrap();
    let (near, _far) = UnixStream::pair().unwrap();
    assert!(
        std::panic::catch_unwind(|| await_decoder_eof(near.as_raw_fd(), Duration::ZERO)).is_err()
    );
}

fn child_arguments_review(name: &str) -> Vec<String> {
    vec![
        "--exact".into(),
        format!("flow::context::failed_enter_driver::tests::review_tests::{name}"),
        "--nocapture".into(),
    ]
}

fn spawn_peer(fx: &mut Fixture, name: &str) -> FailedEnterResources {
    let mut resources = fx.context.failed_enter_resources().unwrap();
    // SAFETY: every caller retains this real child's custody and polls its reap
    // before its test thread returns, including the capped-child fixture.
    unsafe {
        resources.spawn_spu(
            &mut fx.context,
            &std::env::current_exe().unwrap(),
            &child_arguments_review(name),
            &mut fx.clock,
        )
    }
    .unwrap();
    resources
}

fn reap_after_cap(done: &mut FailedEnterFinished) {
    for _ in 0..3000 {
        if done.poll_children() {
            break;
        }
        std::thread::sleep(Duration::from_millis(1));
    }
    assert!(!done.resources().has_unreaped_child());
}

#[test]
fn failed_enter_review_release_then_stall_peer() {
    if !child_mode("failed_enter_review_release_then_stall_peer") {
        return;
    }
    child_release(0);
    // Ensure the exact residency endpoint has really closed. The child then
    // deliberately stays alive, a real reap delay rather than a forged state.
    await_decoder_eof(3, Duration::from_secs(3));
    std::thread::sleep(Duration::from_secs(60));
}

#[test]
fn failed_enter_review_unload_precedes_child_wait_and_keeps_drained_cap_evidence() {
    let mut fx = Fixture::new(config());
    fx.activate();
    let resources = spawn_peer(&mut fx, "failed_enter_review_release_then_stall_peer");
    let first = state_eof(&mut fx);
    let mut owner = start(&mut fx, resources);
    let deadline = owner.deadline().unwrap();
    for _ in 0..3000 {
        assert_ne!(owner.advance(&mut fx.clock), FailedEnterProgress::Closed);
        if owner.trace_drained {
            break;
        }
        std::thread::sleep(Duration::from_millis(1));
    }
    assert_eq!(owner.stage, Stage::Drain);
    assert!(
        owner.trace_drained,
        "trace must drain while actual child still lives"
    );
    let sequence = owner
        .unload_sequence()
        .expect("Unload cannot wait for child exit");
    assert_eq!(owner.resources().spu_child(), Some(ChildState::Running));
    assert_eq!(owner.release_outcome(), ReleaseOutcome::Confirmed);
    let rows = records(&mut fx);
    let unloads: Vec<_> = rows.iter().filter(|r| r["kind"] == "unload").collect();
    assert_eq!(unloads.len(), 1);
    assert_eq!(unloads[0]["payload"]["release"]["spu"], "confirmed");
    assert_eq!(unloads[0]["payload"]["release"]["member"], "unconfirmed");
    fx.clock.advance_to(deadline).unwrap();
    assert_eq!(owner.advance(&mut fx.clock), FailedEnterProgress::Closed);
    let mut done = match owner.finish() {
        Ok(done) => done,
        Err(_) => panic!("closed"),
    };
    assert!(done.abandoned());
    assert!(done.trace_drained());
    assert_eq!(done.unload_sequence(), Some(sequence));
    assert!(done.unload_attempted());
    assert!(done.unload_unavailable().is_none());
    assert!(done.report_unavailable().is_none());
    assert!(done.account_failure().is_none());
    assert!(done.summary_account().is_none());
    assert_eq!(done.failure(), &first);
    assert_eq!(done.release_outcome(), ReleaseOutcome::Confirmed);
    assert!(done.release_send().unwrap().sent());
    assert!(done.release_reply().is_some());
    assert!(
        done.resources()
            .spu
            .as_ref()
            .unwrap()
            .child
            .kill_attempt()
            .is_some()
    );
    assert!(
        records(&mut fx).is_empty(),
        "no record after terminal Unload"
    );
    reap_after_cap(&mut done);
}

pub(super) fn prior_admission() -> LifecycleDirective {
    // The real SPU residency request is Admit (the admin Load verb is another
    // seam). This fixture shape matches existing service tests, not a qualified
    // language/model artifact or an executed model admission.
    LifecycleDirective::Admit {
        instruction: weaver_types::SpuInstruction {
            classify: None,
            decoder: weaver_types::DecoderInstruction {
                model_binding: weaver_types::ModelBinding {
                    artifact: weaver_types::ArtifactRef("fixture-model".into()),
                    devices: vec![weaver_types::DeviceOrdinal(0)],
                },
                residual_readout_election: false,
                field_election: None,
                surprisal_election: false,
                refeed_permission: false,
                column_permission: false,
                tunable_values: Default::default(),
            },
        },
    }
}

fn prior_reply_peer(mode: u8) {
    let mut bytes = [0u8; 8192];
    // SAFETY: actual producer-placed fd3 is owned by this exec fixture.
    let n = unsafe { nix::libc::recv(3, bytes.as_mut_ptr().cast(), bytes.len(), 0) };
    assert!(n > 0);
    let request: OrganEnvelope = serde_json::from_slice(&bytes[..n as usize]).unwrap();
    assert!(matches!(
        request.payload,
        weaver_types::Payload::Directive(LifecycleDirective::Admit { .. })
    ));
    let mut exchange = request.exchange;
    if mode == 1 {
        exchange.ordinal += 1;
    }
    let payload = match mode {
        0 | 1 => weaver_types::Payload::Answer(LifecycleAnswer::Admitted),
        2 => weaver_types::Payload::Answer(LifecycleAnswer::Released),
        3 => weaver_types::Payload::Refusal(weaver_types::LifecycleRefusal::DeviceCannotAdmit),
        4 => weaver_types::Payload::Refusal(weaver_types::LifecycleRefusal::NoResidency),
        _ => panic!("unknown prior fixture mode"),
    };
    let reply = serde_json::to_vec(&OrganEnvelope {
        exchange,
        position: Position::Close,
        payload,
    })
    .unwrap();
    let n = unsafe {
        nix::libc::send(
            3,
            reply.as_ptr().cast(),
            reply.len(),
            nix::libc::MSG_NOSIGNAL,
        )
    };
    assert_eq!(n as usize, reply.len());
    if matches!(mode, 1 | 2 | 4) {
        await_decoder_eof(3, Duration::from_secs(3));
    } else {
        // A genuine refused Admit leaves no residency: the subsequent cleanup
        // attempt receives its own refusal, never a fabricated Released fact.
        child_release(if mode == 3 { 4 } else { 0 });
    }
}

#[test]
fn failed_enter_review_prior_then_release_peer() {
    if child_mode("failed_enter_review_prior_then_release_peer") {
        prior_reply_peer(0);
    }
}
#[test]
fn failed_enter_review_foreign_prior_peer() {
    if child_mode("failed_enter_review_foreign_prior_peer") {
        prior_reply_peer(1);
    }
}

#[test]
fn failed_enter_review_wrong_answer_prior_peer() {
    if child_mode("failed_enter_review_wrong_answer_prior_peer") {
        prior_reply_peer(2);
    }
}
#[test]
fn failed_enter_review_refused_prior_peer() {
    if child_mode("failed_enter_review_refused_prior_peer") {
        prior_reply_peer(3);
    }
}
#[test]
fn failed_enter_review_wrong_refusal_prior_peer() {
    if child_mode("failed_enter_review_wrong_refusal_prior_peer") {
        prior_reply_peer(4);
    }
}

#[test]
fn failed_enter_review_correlated_wrong_operation_keeps_original_and_never_sends_release() {
    for peer in [
        "failed_enter_review_wrong_answer_prior_peer",
        "failed_enter_review_wrong_refusal_prior_peer",
    ] {
        let mut fx = Fixture::new(config());
        fx.activate();
        let mut resources = spawn_peer(&mut fx, peer);
        resources.stage_residency(prior_admission()).unwrap();
        let original = resources.residency_request().unwrap().envelope().clone();
        for _ in 0..3000 {
            resources
                .residency_mut()
                .unwrap()
                .try_send_observed(&mut fx.clock)
                .unwrap();
            resources.residency_mut().unwrap().try_receive().unwrap();
            if resources.residency_mut().unwrap().held().is_some() {
                break;
            }
            std::thread::sleep(Duration::from_millis(1));
        }
        assert!(resources.residency_request().unwrap().send().sent());
        assert!(resources.residency_mut().unwrap().held().is_some());
        let first = state_eof(&mut fx);
        let mut owner = start(&mut fx, resources);
        drive(&mut owner, &mut fx.clock);
        let done = match owner.finish() {
            Ok(done) => done,
            Err(_) => panic!("closed"),
        };
        assert_eq!(done.failure(), &first);
        assert_eq!(done.release_outcome(), ReleaseOutcome::Unconfirmed);
        assert_eq!(done.prior_residency().unwrap().envelope(), &original);
        let prior = done
            .prior_residency_reply()
            .expect("retain the actual correlated wrong packet");
        assert_eq!(prior.envelope().exchange, original.exchange);
        assert_eq!(
            serde_json::from_slice::<OrganEnvelope>(prior.raw_bytes()).unwrap(),
            *prior.envelope()
        );
        assert!(!matches!(
            prior.envelope().payload,
            weaver_types::Payload::Answer(LifecycleAnswer::Admitted)
        ));
        assert!(!done.release_send().unwrap().sent());
        assert!(done.release_reply().is_none());
        assert_eq!(
            done.release_unavailable(),
            Some("prior Admit reply was not Admitted or an Admit refusal")
        );
        assert!(matches!(
            done.resources().spu_child(),
            Some(ChildState::Reaped(nix::sys::wait::WaitStatus::Exited(_, 0)))
        ));
    }
}

#[test]
fn failed_enter_review_foreign_prior_reply_cannot_be_consumed_as_the_real_earlier_exchange() {
    let mut fx = Fixture::new(config());
    fx.activate();
    let mut resources = spawn_peer(&mut fx, "failed_enter_review_foreign_prior_peer");
    resources.stage_residency(prior_admission()).unwrap();
    let original = resources.residency_request().unwrap().envelope().clone();
    for _ in 0..3000 {
        resources
            .residency_mut()
            .unwrap()
            .try_send_observed(&mut fx.clock)
            .unwrap();
        resources.residency_mut().unwrap().try_receive().unwrap();
        if resources.residency_mut().unwrap().held().is_some() {
            break;
        }
        std::thread::sleep(Duration::from_millis(1));
    }
    assert!(resources.residency_request().unwrap().send().sent());
    assert!(resources.residency_mut().unwrap().held().is_some());
    state_eof(&mut fx);
    let mut owner = start(&mut fx, resources);
    drive(&mut owner, &mut fx.clock);
    let done = match owner.finish() {
        Ok(done) => done,
        Err(_) => panic!("closed"),
    };
    assert_eq!(done.release_outcome(), ReleaseOutcome::Unconfirmed);
    assert_eq!(done.prior_residency().unwrap().envelope(), &original);
    assert!(done.prior_residency_reply().is_none());
    assert!(!done.release_send().unwrap().sent());
    assert!(done.release_reply().is_some());
    assert!(done.release_unavailable().is_some());
    assert!(matches!(
        done.resources().spu_child(),
        Some(ChildState::Reaped(nix::sys::wait::WaitStatus::Exited(_, 0)))
    ));
}

#[test]
fn failed_enter_review_exact_prior_reply_retains_request_and_continues_actual_release() {
    for peer in [
        "failed_enter_review_prior_then_release_peer",
        "failed_enter_review_refused_prior_peer",
    ] {
        let mut fx = Fixture::new(config());
        fx.activate();
        let mut resources = spawn_peer(&mut fx, peer);
        resources.stage_residency(prior_admission()).unwrap();
        let original = resources.residency_request().unwrap().envelope().clone();
        for _ in 0..3000 {
            resources
                .residency_mut()
                .unwrap()
                .try_send_observed(&mut fx.clock)
                .unwrap();
            resources.residency_mut().unwrap().try_receive().unwrap();
            if resources.residency_mut().unwrap().held().is_some() {
                break;
            }
            std::thread::sleep(Duration::from_millis(1));
        }
        assert!(resources.residency_request().unwrap().send().sent());
        assert!(resources.residency_mut().unwrap().held().is_some());
        let first = state_eof(&mut fx);
        let mut owner = start(&mut fx, resources);
        drive(&mut owner, &mut fx.clock);
        assert_eq!(owner.failure(), &first);
        let refused = peer == "failed_enter_review_refused_prior_peer";
        assert_eq!(
            owner.release_outcome(),
            if refused {
                ReleaseOutcome::Unconfirmed
            } else {
                ReleaseOutcome::Confirmed
            }
        );
        let done = match owner.finish() {
            Ok(done) => done,
            Err(_) => panic!("closed"),
        };
        assert!(!done.abandoned());
        assert!(done.trace_drained());
        assert_eq!(done.prior_residency().unwrap().envelope(), &original);
        assert!(done.prior_residency().unwrap().send().sent());
        let prior = done.prior_residency_reply().unwrap();
        if peer == "failed_enter_review_refused_prior_peer" {
            assert!(matches!(
                prior.envelope().payload,
                weaver_types::Payload::Refusal(weaver_types::LifecycleRefusal::DeviceCannotAdmit)
            ));
        } else {
            assert!(matches!(
                prior.envelope().payload,
                weaver_types::Payload::Answer(LifecycleAnswer::Admitted)
            ));
        }
        assert_eq!(prior.envelope().exchange, original.exchange);
        assert_eq!(
            serde_json::from_slice::<OrganEnvelope>(prior.raw_bytes()).unwrap(),
            *prior.envelope()
        );
        let released = done.release_reply().unwrap();
        assert_ne!(released.envelope().exchange, original.exchange);
        assert!(
            done.release_send()
                .unwrap()
                .matches_exchange(&released.envelope().exchange)
        );
        assert!(done.release_send().unwrap().sent());
        if refused {
            assert!(matches!(
                released.envelope().payload,
                weaver_types::Payload::Refusal(weaver_types::LifecycleRefusal::OutOfOrder)
            ));
            assert!(done.release_unavailable().is_some());
        } else {
            assert!(matches!(
                released.envelope().payload,
                weaver_types::Payload::Answer(LifecycleAnswer::Released)
            ));
            assert!(done.release_unavailable().is_none());
        }
        assert!(matches!(
            done.resources().spu_child(),
            Some(ChildState::Reaped(nix::sys::wait::WaitStatus::Exited(_, 0)))
        ));
    }
}

#[test]
fn failed_enter_review_precleanup_release_refusal_preserves_admit_and_once_only_cleanup_release() {
    let mut fx = Fixture::new(config());
    fx.activate();
    let mut resources = spawn_peer(&mut fx, "failed_enter_review_prior_then_release_peer");
    let ordinal = resources.spu.as_ref().unwrap().next_ordinal;
    let (original, reason) = resources
        .stage_residency(LifecycleDirective::Release)
        .unwrap_err();
    assert_eq!(original, LifecycleDirective::Release);
    assert_eq!(reason, "pre-cleanup residency requires Admit");
    assert_eq!(resources.spu.as_ref().unwrap().next_ordinal, ordinal);
    assert!(resources.residency_request().is_none());
    resources.stage_residency(prior_admission()).unwrap();
    let admit = resources.residency_request().unwrap().envelope().clone();
    let next = resources.spu.as_ref().unwrap().next_ordinal;
    let request = resources.residency_request().unwrap() as *const ResidencyRequest;
    let (original, _) = resources
        .stage_residency(LifecycleDirective::Release)
        .unwrap_err();
    assert_eq!(original, LifecycleDirective::Release);
    assert_eq!(resources.spu.as_ref().unwrap().next_ordinal, next);
    assert_eq!(
        resources.residency_request().unwrap() as *const ResidencyRequest,
        request
    );
    assert_eq!(resources.residency_request().unwrap().envelope(), &admit);
    for _ in 0..3000 {
        resources
            .residency_mut()
            .unwrap()
            .try_send_observed(&mut fx.clock)
            .unwrap();
        resources.residency_mut().unwrap().try_receive().unwrap();
        if resources.residency_mut().unwrap().held().is_some() {
            break;
        }
        std::thread::sleep(Duration::from_millis(1));
    }
    assert!(resources.residency_request().unwrap().send().sent());
    assert!(resources.residency_mut().unwrap().held().is_some());
    state_eof(&mut fx);
    let mut owner = start(&mut fx, resources);
    drive(&mut owner, &mut fx.clock);
    let done = match owner.finish() {
        Ok(done) => done,
        Err(_) => panic!("closed"),
    };
    assert_eq!(done.release_outcome(), ReleaseOutcome::Confirmed);
    assert_eq!(done.prior_residency().unwrap().envelope(), &admit);
    assert_eq!(
        done.release_reply().unwrap().envelope().exchange.ordinal,
        next.unwrap()
    );
    assert!(matches!(
        done.resources().spu_child(),
        Some(ChildState::Reaped(nix::sys::wait::WaitStatus::Exited(_, 0)))
    ));
}

#[test]
fn failed_enter_review_reversed_clock_closes_without_terminal_or_losing_first_cause() {
    let mut fx = Fixture::new(config());
    fx.activate();
    let resources = fx.context.failed_enter_resources().unwrap();
    let first = state_eof(&mut fx);
    let mut owner = start(&mut fx, resources);
    let last = owner.context.last_ns;
    let deadline = owner.deadline();
    assert!(last > 0);
    let mut reversed = FakePumpIo::new(last - 1);
    assert_eq!(owner.advance(&mut reversed), FailedEnterProgress::Closed);
    let done = match owner.finish() {
        Ok(done) => done,
        Err(_) => panic!("closed"),
    };
    assert_eq!(done.failure(), &first);
    assert!(done.abandoned());
    assert!(!done.trace_drained());
    assert!(!done.unload_attempted());
    assert!(done.unload_sequence().is_none());
    assert_eq!(done.context().last_ns, last);
    assert_eq!(done.cleanup.deadline(), deadline);
    let clock = done
        .context()
        .flow
        .halt()
        .unwrap()
        .secondary()
        .iter()
        .find(|s| s.detail == "failed cleanup clock reversed")
        .expect("actual reversed-clock evidence");
    assert_eq!(clock.observed_ns, Some((last - 1).into()));
    assert!(records(&mut fx).iter().all(|row| row["kind"] != "unload"));
}

#[test]
fn failed_enter_review_secondary_repeats_preserve_first_observation_and_distinct_details() {
    let mut fx = Fixture::new(config());
    fx.activate();
    let resources = fx.context.failed_enter_resources().unwrap();
    let first = state_eof(&mut fx);
    let mut owner = start(&mut fx, resources);
    // Defensive helper probe: actual admitted cleanup, repeated synthetic error
    // details. It does not claim a currently reachable broken account owner.
    let original = owner.context.flow.halt().unwrap().secondary().len();
    let observed = owner.context.last_ns;
    owner.secondary("same account unavailable");
    for tick in 1..=100 {
        fx.clock.advance_to(observed + tick).unwrap();
        assert!(owner.clock(&mut fx.clock));
        owner.secondary("same account unavailable");
    }
    owner.secondary("different account unavailable");
    let rows = owner.context.flow.halt().unwrap().secondary();
    assert_eq!(rows.len(), original + 2);
    assert_eq!(rows[original].observed_ns, Some(observed.into()));
    assert_eq!(rows[original].detail, "same account unavailable");
    assert_eq!(rows[original + 1].detail, "different account unavailable");
    assert_eq!(owner.failure(), &first);
    drive(&mut owner, &mut fx.clock);
}

#[test]
fn failed_enter_review_account_wait_preserves_either_and_tighter_bound() {
    // Direct defensive helper control: cleanup's production clock rejects a
    // missing inherited cap before it can construct an account continuation.
    for (caller, inherited, expected) in [
        (None, None, None),
        (Some(7), None, Some(7)),
        (None, Some(9), Some(9)),
        (Some(7), Some(9), Some(7)),
        (Some(11), Some(9), Some(9)),
    ] {
        struct WaitProbe(Option<Option<u64>>);
        impl PumpIo for WaitProbe {
            fn now(&mut self) -> std::io::Result<u64> {
                Ok(1)
            }
            fn wait(
                &mut self,
                _: &[WaitInterest<'_>],
                bound: Option<u64>,
            ) -> std::io::Result<crate::flow::clock::WaitOutcome> {
                self.0 = Some(bound);
                Ok(crate::flow::clock::WaitOutcome::Timeout)
            }
        }
        let mut probe = WaitProbe(None);
        let mut account = AccountClock {
            io: &mut probe,
            last: 1,
            deadline: inherited,
            unavailable: false,
        };
        account.wait(&[], caller).unwrap();
        assert!(!account.unavailable);
        assert_eq!(probe.0, Some(expected));
    }
}
