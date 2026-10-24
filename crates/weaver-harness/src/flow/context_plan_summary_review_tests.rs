//! Followups to Planner review 5623894517. These drive the actual private
//! owners and socket/recorder boundaries, with explicit fault injection below.
use super::*;

/// A bounded fake readiness tick for every supplied leg. A withheld decode leg
/// is deliberately absent; actual nonblocking I/O still decides whether a source
/// made progress. This never manufactures HUP or advances to the inherited cap.
pub(super) struct SummaryReadyClock<'a>(pub(super) &'a mut FakePumpIo);
impl PumpIo for SummaryReadyClock<'_> {
    fn now(&mut self) -> io::Result<u64> {
        self.0.now()
    }
    fn wait(
        &mut self,
        interests: &[WaitInterest<'_>],
        deadline: Option<u64>,
    ) -> io::Result<WaitOutcome> {
        let at = self.0.now()? + 1;
        let ready = interests
            .iter()
            .enumerate()
            .filter_map(|(index, interest)| {
                let events = interest.events & (PollFlags::POLLIN | PollFlags::POLLOUT);
                (!events.is_empty()).then_some(ReadyFd { index, events })
            })
            .collect();
        self.0.schedule(at, ready)?;
        self.0.wait(interests, deadline)
    }
}

fn stage_generation(decode: &mut DecodePort, bytes: usize) -> TokenDirective {
    let request = TokenDirective::AppendAndGenerate {
        turn: weaver_types::TurnKey("summary-review-round".into()),
        delta: vec![Message {
            role: Role::User,
            content: vec![ContentBlock::Text {
                text: "x".repeat(bytes),
            }],
        }],
        tool_schemas: vec![],
    };
    assert!(matches!(
        decode.stage(request.clone()).unwrap(),
        Stage::Queued
    ));
    request
}

#[test]
fn plan_summary_effect_before_boot_returns_original_without_admitting_a_dead_end_duty() {
    for name in ["authorize", "close"] {
        let mut fx = SummaryFixture::new(vec![]);
        let turn = (name == "close").then_some("exact-effect-turn");
        let event = effect(fx.seed(), name, turn);
        let original = format!("{event:?}");
        let run = fx.run();
        let bound = AbsoluteBound::after(run.clock.now().unwrap(), 1_000_000).unwrap();
        run.context
            .arm(PumpDeadline::Decision, bound, &mut run.clock)
            .unwrap();
        let error = run
            .context
            .begin_filing(event, PumpDeadline::Decision, &mut run.clock)
            .unwrap_err();
        assert_eq!(error.refusal, FilingBeginRefusal::Busy);
        assert!(error.failure.is_none());
        assert_eq!(format!("{:?}", error.event), original);
        assert!(run.context.filing.is_none());
        assert!(!run.context.flow.has_filing_consumer());
        assert!(run.context.waiting_content.is_none());
        assert!(run.context.pending_author().is_none());
        assert!(run.context.plan_cache.required.is_none());
        assert_eq!(
            run.context.committed_plan_summary(),
            Err(PlanSummaryUnavailable::Uninitialized)
        );
        assert_eq!(
            run.context.complete_wait(PumpDeadline::Decision),
            Some(bound)
        );
        let snapshot = fx.snapshot();
        fx.boot(Some(snapshot));
        // The exact returned input can now be admitted and actually filed.
        let filing = fx.file(error.event, false);
        assert_eq!(fx.files.len(), 1);
        assert_eq!(committed_position(&filing).block_id, fx.files[0].block_id);
    }
}

#[test]
fn plan_summary_unsent_generation_does_not_starve_actual_refresh_state_traffic() {
    for boot in [true, false] {
        let mut fx = SummaryFixture::new(vec![]);
        let mut snapshot = fx.snapshot();
        if boot {
            fx.begin_boot();
        } else {
            fx.boot(Some(snapshot.clone()));
            let filing = fx.file(effect(fx.seed(), "authorize", None), false);
            snapshot.last_source = committed_position(&filing);
            fx.begin_effect(filing);
        }
        let (mut decode, peer) = fx._decode.take().unwrap();
        let append = stage_generation(&mut decode, 7);
        let raw = summary_bytes(Some(snapshot));
        let mut read_seen = false;
        let mut release_seen = false;
        let mut completed = false;
        for _ in 0..256 {
            let run = fx.run();
            let mut io = SummaryReadyClock(&mut run.clock);
            let mut supplied = PumpPorts {
                lifecycle: LifecyclePorts::default(),
                gate: None,
                decode: Some(&mut decode),
                label: None,
                io: &mut io,
            };
            run.context.pump_once(&mut supplied).unwrap();
            let progress = run.context.advance_plan_summary(&mut supplied).unwrap();
            assert!(
                supplied.decode.is_some(),
                "caller keeps the exact deferred port"
            );
            assert!(decode.generation_not_started());
            assert!(receive(peer.as_fd()).is_none());
            let run = fx.run();
            let mut line = [0u8; 65536];
            let received = match run.state.read(&mut line) {
                Ok(count) => {
                    Some(s::decode_state_line(&line[..count], s::FrameDirection::Request).unwrap())
                }
                Err(error) if error.kind() == io::ErrorKind::WouldBlock => None,
                other => panic!("actual summary request read: {other:?}"),
            };
            if let Some(request) = received {
                let answer = match &request {
                    s::StateFrame::Control {
                        frame:
                            s::StateControl::Read {
                                selector: s::ReadSelector::PlanCurrent {},
                            },
                        ..
                    } => {
                        assert!(!read_seen);
                        read_seen = true;
                        page(912, 0, raw.len(), &raw)
                    }
                    s::StateFrame::Control {
                        frame: s::StateControl::Release { snapshot_id },
                        ..
                    } => {
                        assert!(read_seen && !release_seen);
                        release_seen = true;
                        assert_eq!(snapshot_id.0, 912);
                        s::StateControlAnswer::Released {
                            snapshot_id: *snapshot_id,
                        }
                    }
                    other => panic!("unexpected summary request {other:?}"),
                };
                let response = fx.boot.member.answer(&request, answer);
                // The scripted member measured work ending two ticks after
                // the current harness time. Receive it only after that work.
                let received_at = fx.boot.member.now() + 3;
                fx.run().clock.advance_to(received_at).unwrap();
                fx.run()
                    .state
                    .write_all(
                        &s::encode_state_line(&response, s::FrameDirection::Response).unwrap(),
                    )
                    .unwrap();
            }
            if progress == PlanSummaryProgress::Complete {
                completed = true;
                break;
            }
        }
        assert!(read_seen && release_seen && completed);
        let run = fx.run();
        let proof = run
            .context
            .take_plan_summary(&mut run.clock)
            .unwrap()
            .unwrap();
        let read = match &proof {
            PlanSummaryCompletion::Boot(proof) => proof.read(),
            PlanSummaryCompletion::Effect(proof) => proof.read(),
            other => panic!("completed refresh lost its disposition {other:?}"),
        };
        assert_eq!(read.raw_bytes(), raw);
        let mut sent = None;
        for _ in 0..16 {
            pump_summary_decode(&mut fx, &mut decode).unwrap();
            sent = receive(peer.as_fd());
            if sent.is_some() {
                break;
            }
        }
        assert_eq!(
            serde_json::from_slice::<TokenDirective>(&sent.unwrap()).unwrap(),
            append
        );
    }
}

#[test]
fn plan_summary_partly_sent_generation_drains_exact_segmented_request_while_cache_is_pending() {
    let mut fx = SummaryFixture::new(vec![]);
    let snapshot = fx.snapshot();
    fx.boot(Some(snapshot));
    let (mut decode, peer) = fx._decode.take().unwrap();
    let request = stage_generation(&mut decode, weaver_types::MAX_ENVELOPE_BYTES * 2 + 19);
    let expected = serde_json::to_vec(&request).unwrap();
    // Only the real preamble crosses before the effect starts. Remaining body
    // datagrams are an already-started exchange, not a new generation grant.
    pump_summary_decode(&mut fx, &mut decode).unwrap();
    let head: serde_json::Value = serde_json::from_slice(&receive(peer.as_fd()).unwrap()).unwrap();
    assert_eq!(head["bytes"], expected.len());
    assert!(!decode.generation_not_started());
    assert!(decode.send_pending());
    let filing = fx.file(effect(fx.seed(), "authorize", None), false);
    fx.begin_effect(filing);
    let mut body = Vec::new();
    for _ in 0..16 {
        pump_summary_decode(&mut fx, &mut decode).unwrap();
        if let Some(packet) = receive(peer.as_fd()) {
            body.extend(packet);
        }
        if !decode.send_pending() {
            break;
        }
    }
    assert!(!decode.send_pending());
    assert_eq!(body, expected);
    assert_eq!(
        fx.run().context.committed_plan_summary(),
        Err(PlanSummaryUnavailable::Pending)
    );
}

#[test]
fn plan_summary_unflagged_author_cannot_file_a_plan_effect_or_step_outcome() {
    for name in ["authorize", "close"] {
        let mut fx = SummaryFixture::new(vec![]);
        let snapshot = fx.snapshot();
        fx.boot(Some(snapshot));
        let event = effect(fx.seed(), name, None);
        let original = format!("{event:?}");
        let before = fx.records(event.kind).len();
        let run = fx.run();
        assert_eq!(
            run.context
                .author_and_wait(&mut ports(&mut run.clock), event)
                .unwrap_err(),
            RunFailure::PlanSummaryOwned
        );
        assert_eq!(
            format!("{:?}", run.context.held_author().unwrap()),
            original
        );
        assert!(run.context.pending_author().is_none());
        assert!(!run.context.flow.has_filing_consumer());
        assert!(run.context.flow.state().outstanding().is_empty());
        assert!(run.context.plan_cache.required.is_none());
        assert_eq!(
            fx.records(if name == "close" {
                Kind::StepOutcome
            } else {
                Kind::PlanEvent
            })
            .len(),
            before
        );
    }
}

#[test]
fn plan_summary_completed_refresh_keeps_its_exact_outcome_when_stop_revokes_future_use() {
    for boot in [true, false] {
        let mut fx = SummaryFixture::new(vec![]);
        let mut snapshot = fx.snapshot();
        if boot {
            fx.begin_boot();
        } else {
            fx.boot(Some(snapshot.clone()));
            let filing = fx.file(effect(fx.seed(), "authorize", None), false);
            snapshot.last_source = committed_position(&filing);
            fx.begin_effect(filing);
        }
        let raw = fx.release(Some(snapshot));
        finish_summary_without_take(&mut fx);
        let (admin, peer) = OrganChannel::pair().unwrap();
        let mut admin = LifecyclePeer::from_channel(admin).unwrap();
        let stop = directive(weaver_types::LifecycleDirective::Stop {
            cause: weaver_types::Cause { uid: 0 },
        });
        send_bytes(peer.as_fd(), &serde_json::to_vec(&stop).unwrap());
        let run = fx.run();
        let mut io = SummaryReadyClock(&mut run.clock);
        run.context
            .pump_once(&mut PumpPorts {
                lifecycle: LifecyclePorts {
                    listener: None,
                    peer: Some(&mut admin),
                    residency: None,
                    dial: None,
                    leave_peer: None,
                },
                gate: None,
                decode: None,
                label: None,
                io: &mut io,
            })
            .unwrap();
        assert!(admin.held().is_some());
        if boot {
            assert_eq!(
                run.context.take_plan_summary(&mut run.clock).unwrap_err(),
                RunFailure::PlanSummaryOwned
            );
            let owner = run.context.plan_summary.as_ref().unwrap();
            assert!(owner.disposition_for_test().complete);
            assert!(!owner.disposition_for_test().cancelled);
        } else {
            let Some(PlanSummaryCompletion::Effect(proof)) =
                run.context.take_plan_summary(&mut run.clock).unwrap()
            else {
                panic!("Stop rewrote completed effect evidence");
            };
            assert_eq!(proof.read().raw_bytes(), raw);
            assert_eq!(
                run.context.committed_plan_summary(),
                Err(PlanSummaryUnavailable::Revoked)
            );
        }
        assert_eq!(admin.take_completion().unwrap().unwrap().envelope, stop);
        let (mut decode, peer) = fx._decode.take().unwrap();
        stage_generation(&mut decode, 1);
        pump_summary_decode(&mut fx, &mut decode).unwrap();
        assert!(decode.generation_not_started());
        assert!(receive(peer.as_fd()).is_none());
        if boot {
            let account = rollback_boot_summary(&mut fx);
            assert_eq!(account.recorded_read().unwrap().raw_bytes(), raw);
            assert!(account.disposition_for_test().complete);
            assert!(!account.disposition_for_test().cancelled);
        }
    }
}

#[test]
fn plan_summary_cut_filing_refresh_borrows_its_exact_wait_through_recorded() {
    let mut fx = SummaryFixture::new(vec![]);
    let mut snapshot = fx.snapshot();
    fx.boot(Some(snapshot.clone()));
    let event = effect(fx.seed(), "authorize", None);
    let run = fx.run();
    let cut = AbsoluteBound::after(run.clock.now().unwrap(), 1_000_000).unwrap();
    run.context
        .arm(PumpDeadline::Cut, cut, &mut run.clock)
        .unwrap();
    run.context
        .begin_filing(event, PumpDeadline::Cut, &mut run.clock)
        .unwrap();
    let filing = finish_existing_filing(&mut fx);
    assert_eq!(filing.wait_key(), PumpDeadline::Cut);
    assert_eq!(filing.wait_bound(), cut);
    snapshot.last_source = committed_position(&filing);
    fx.begin_effect(filing);
    assert_eq!(fx.run().context.complete_wait(PumpDeadline::Cut), None);
    let raw = fx.release(Some(snapshot));
    let PlanSummaryCompletion::Effect(proof) = fx.finish() else {
        panic!("Cut summary did not settle");
    };
    assert_eq!(proof.read().raw_bytes(), raw);
    assert_eq!(fx.run().context.complete_wait(PumpDeadline::Cut), Some(cut));
}

#[test]
fn plan_summary_boot_admission_returns_actual_proof_while_foreign_wait_is_held() {
    for key in [
        PumpDeadline::Decision,
        PumpDeadline::Cut,
        PumpDeadline::Gate,
        PumpDeadline::StateControl,
    ] {
        let mut fx = SummaryFixture::new(vec![]);
        let proof = fx.repaired.take().unwrap();
        let run = fx.run();
        let other = AbsoluteBound::after(run.clock.now().unwrap(), 1_000_000).unwrap();
        run.context.arm(key, other, &mut run.clock).unwrap();
        let error = run
            .context
            .begin_boot_plan_summary(proof, &mut run.clock)
            .unwrap_err();
        assert_eq!(error.refusal, PlanSummaryBeginRefusal::Busy);
        assert!(error.failure.is_none());
        assert!(run.context.plan_summary.is_none());
        assert!(run.context.read.is_none());
        assert_eq!(run.context.complete_wait(key), Some(other));
        run.context
            .begin_boot_plan_summary(error.prerequisite, &mut run.clock)
            .unwrap();
        let snapshot = fx.snapshot();
        fx.release(Some(snapshot));
        assert!(matches!(fx.finish(), PlanSummaryCompletion::Boot(_)));
    }
}

#[test]
fn plan_summary_boot_rejects_real_proof_after_enter_bound_identity_drift() {
    let mut fx = SummaryFixture::new(vec![]);
    let proof = fx.repaired.take().unwrap();
    let run = fx.run();
    let enter = run.context.deadlines.get(PumpDeadline::Enter).unwrap();
    // Deliberate registry corruption probes the independent proof comparison;
    // ordinary complete_wait/arm guards forbid renewing an owner's live bound.
    run.context.deadlines.disarm(PumpDeadline::Enter);
    let other = AbsoluteBound::from_absolute(enter.began_ns(), enter.deadline_ns() + 1).unwrap();
    run.context
        .deadlines
        .arm(PumpDeadline::Enter, other)
        .unwrap();
    let error = run
        .context
        .begin_boot_plan_summary(proof, &mut run.clock)
        .unwrap_err();
    assert_eq!(error.refusal, PlanSummaryBeginRefusal::WrongProof);
    assert!(error.failure.is_none());
    assert!(run.context.plan_summary.is_none());
    assert!(run.context.read.is_none());
    run.context.deadlines.disarm(PumpDeadline::Enter);
    run.context
        .deadlines
        .arm(PumpDeadline::Enter, enter)
        .unwrap();
    run.context
        .begin_boot_plan_summary(error.prerequisite, &mut run.clock)
        .unwrap();
    let snapshot = fx.snapshot();
    fx.release(Some(snapshot));
    assert!(matches!(fx.finish(), PlanSummaryCompletion::Boot(_)));
}

#[test]
fn plan_summary_owned_refresh_detects_replaced_inherited_bound_before_read_admission() {
    let mut fx = SummaryFixture::new(vec![]);
    let snapshot = fx.snapshot();
    fx.boot(Some(snapshot));
    let filing = fx.file(effect(fx.seed(), "authorize", None), false);
    let bound = filing.wait_bound();
    fx.begin_effect(filing);
    let run = fx.run();
    // A registry fault, not a claimed normal API path: the exact owner must
    // still detect it before it can convert a renewed window into a query.
    run.context.deadlines.disarm(PumpDeadline::Decision);
    run.context
        .deadlines
        .arm(
            PumpDeadline::Decision,
            AbsoluteBound::from_absolute(bound.began_ns(), bound.deadline_ns() + 1).unwrap(),
        )
        .unwrap();
    let failure = fx.advance().unwrap_err();
    assert!(fx.run().context.failure().is_some());
    assert_eq!(fx.run().context.failure(), Some(&failure));
    assert!(fx.run().context.read.is_none());
    assert!(fx.run().context.flow.state().outstanding().is_empty());
    assert_eq!(
        fx.run().context.committed_plan_summary(),
        Err(PlanSummaryUnavailable::Failed)
    );
}

#[test]
fn plan_summary_boot_shutdown_syscall_error_is_retained_as_incomplete_evidence() {
    let mut fx = SummaryFixture::new(vec![]);
    fx.begin_boot();
    let request = fx.next_request();
    fx.boot
        .member
        .respond(&request, refused(s::ReadRefusalCode::ResultUnavailable));
    finish_summary_without_take(&mut fx);
    let run = fx.run();
    let fd = run.context.flow.state().fd().as_raw_fd();
    let replacement = std::fs::File::open("/dev/null").unwrap();
    // Inject an actual shutdown ENOTSOCK without closing/reusing an unowned fd:
    // SAFETY: dup2 atomically replaces only this fixture's descriptor; its owner still
    // closes exactly that descriptor. No test-only success/error flag is used.
    assert_eq!(unsafe { nix::libc::dup2(replacement.as_raw_fd(), fd) }, fd);
    let account = rollback_boot_summary(&mut fx);
    assert!(account.refusal().is_some());
    assert!(account.recorded_read().is_none());
    assert!(
        account
            .incomplete()
            .is_some_and(|detail| detail.contains("summary channel close:"))
    );
    assert!(fx.run().context.flow.standing_state().is_none());
}

#[test]
fn plan_summary_stopped_account_does_not_resume_or_retire_a_foreign_valid_ticket() {
    let mut fx = SummaryFixture::new(vec![]);
    let mut snapshot = fx.snapshot();
    fx.boot(Some(snapshot.clone()));
    let filing = fx.file(effect(fx.seed(), "authorize", None), false);
    snapshot.last_source = committed_position(&filing);
    fx.begin_effect(filing);
    released_summary_recall(&mut fx, Some(snapshot));
    fx.boot.member.fill_trace();
    assert_eq!(fx.advance().unwrap(), PlanSummaryProgress::Pending);
    let run = fx.run();
    let original = run.context.pending_author().unwrap();
    let ticket = original.ticket;
    let event = original.event.clone();
    assert!(run.context.barrier.abandon_boot_author(ticket));
    assert!(matches!(
        run.context.barrier.submit_authored_with_clock(
            event,
            RecordPurpose::Observation,
            &run.context.author,
            1,
            &mut run.context.inbox,
            &mut run.context.budget,
            &mut || run.clock.now()
        ),
        RecordProgress::Pending
    ));
    let foreign = run.context.pending_author().unwrap().ticket;
    assert_ne!(foreign, ticket);
    run.context.fail(
        &mut run.clock,
        FlowHaltReason::ProtocolFailed,
        HaltPoint::Barrier,
        "independent failure after a foreign ticket replaced the summary ticket",
    );
    run.context
        .freeze_halt_account(ExternalFacts {
            outstanding_pairs: vec![],
            capture: None,
            wait: run.context.halt_wait().cloned(),
            input_id: None,
        })
        .unwrap();
    run.context.begin_halt_report().unwrap();
    run.context.advance_halt_trace(&mut run.clock).unwrap();
    fx.drain();
    let before = fx.records(Kind::Recall).len();
    let run = fx.run();
    assert_eq!(
        run.context
            .advance_plan_summary_account(&mut run.clock)
            .unwrap(),
        PlanSummaryProgress::Complete
    );
    assert_eq!(run.context.pending_author().unwrap().ticket, foreign);
    let account = run.context.take_stopped_plan_summary().unwrap().unwrap();
    assert!(account.recorded_read().is_none());
    assert_eq!(
        account.incomplete(),
        Some("summary recall lost its exact stopped ticket")
    );
    assert_eq!(fx.records(Kind::Recall).len(), before);
}

#[test]
fn plan_summary_actual_committed_foreign_seed_effect_faults_before_refresh() {
    let mut fx = SummaryFixture::new(vec![]);
    let snapshot = fx.snapshot();
    let previous = fx.boot(Some(snapshot)).summary().cloned();
    let foreign_seed = "ffffffffffffffffffffffffffffffff";
    assert_ne!(foreign_seed, fx.seed());
    fx.begin_file(effect(foreign_seed, "authorize", None));
    let mut failure = None;
    for _ in 0..256 {
        let run = fx.run();
        if let Err(error) = run.context.advance_filing(&mut ports(&mut run.clock)) {
            failure = Some(error);
            break;
        }
        fx.classifier(false);
        fx.transport(TransportSource::State);
        let run = fx.run();
        let mut bytes = [0; 65536];
        let count = match run.state.read(&mut bytes) {
            Ok(count) => count,
            Err(error) if error.kind() == io::ErrorKind::WouldBlock => continue,
            other => panic!("foreign seed filing request: {other:?}"),
        };
        let request = s::decode_state_line(&bytes[..count], s::FrameDirection::Request).unwrap();
        let s::StateFrame::File { filing, .. } = &request else {
            panic!("expected actual effect filing");
        };
        fx.files.push(filing.clone());
        let observation = operation(
            &request,
            fx.boot.member.next_operation,
            fx.boot.member.now() + 1,
        );
        fx.boot.member.next_operation += 1;
        if let Err(error) = fx.boot.member.receive(&s::StateFrame::Filed {
            common: request.common().clone(),
            block_id: filing.block_id.clone(),
            observation,
        }) {
            failure = Some(error);
            break;
        }
    }
    assert!(
        failure.is_some(),
        "accepted foreign-seed receipt must fault"
    );
    assert_eq!(
        fx.files.len(),
        1,
        "the real scripted member accepted the foreign effect"
    );
    assert!(
        fx.run()
            .context
            .flow
            .filing_receipts()
            .unwrap()
            .recorded_dispositions()
            .any(|disposition| matches!(disposition, SourceDisposition::Committed(_)))
    );
    assert_eq!(
        fx.run().context.plan_cache.invalid,
        Some("plan effect receipt has another plan seed")
    );
    assert_eq!(fx.run().context.previous_plan_summary(), previous.as_ref());
    assert_eq!(
        fx.run().context.committed_plan_summary(),
        Err(PlanSummaryUnavailable::Failed)
    );
    assert!(fx.run().context.read.is_none());
}

#[test]
fn plan_summary_explicit_cancel_after_completion_revokes_use_without_rewriting_the_outcome() {
    let mut fx = SummaryFixture::new(vec![]);
    let mut snapshot = fx.snapshot();
    fx.boot(Some(snapshot.clone()));
    let filing = fx.file(effect(fx.seed(), "authorize", None), false);
    snapshot.last_source = committed_position(&filing);
    fx.begin_effect(filing);
    let raw = fx.release(Some(snapshot));
    finish_summary_without_take(&mut fx);
    let run = fx.run();
    run.context.cancel_plan_summary(&mut run.clock).unwrap();
    let Some(PlanSummaryCompletion::Effect(proof)) =
        run.context.take_plan_summary(&mut run.clock).unwrap()
    else {
        panic!("explicit cancellation cannot rewrite a completed refresh");
    };
    assert_eq!(proof.read().raw_bytes(), raw);
    assert_eq!(
        run.context.committed_plan_summary(),
        Err(PlanSummaryUnavailable::Revoked)
    );
}

#[test]
fn plan_summary_unstarted_effect_stop_returns_input_and_revokes_only_future_use() {
    let mut fx = SummaryFixture::new(vec![]);
    let snapshot = fx.snapshot();
    let previous = fx.boot(Some(snapshot)).summary().cloned();
    let event = effect(fx.seed(), "authorize", None);
    let original = format!("{event:?}");
    fx.begin_file(event);
    let run = fx.run();
    run.context.cancel_filing();
    let cancellation = run.context.take_cancelled_filing().unwrap();
    assert!(cancellation.sequence.is_none());
    assert_eq!(format!("{:?}", cancellation.event.unwrap()), original);
    assert!(run.context.plan_cache.required.is_none());
    assert!(run.context.flow.state().outstanding().is_empty());
    assert!(run.context.pending_author().is_none());
    assert_eq!(run.context.previous_plan_summary(), previous.as_ref());
    assert_eq!(
        run.context.committed_plan_summary(),
        Err(PlanSummaryUnavailable::Revoked)
    );
    assert!(fx.files.is_empty());
    let (mut decode, peer) = fx._decode.take().unwrap();
    stage_generation(&mut decode, 1);
    pump_summary_decode(&mut fx, &mut decode).unwrap();
    assert!(decode.generation_not_started());
    assert!(receive(peer.as_fd()).is_none());
}

#[test]
fn plan_summary_read_accepted_before_recoverable_yield_resumes_same_owner_without_restarting() {
    let mut witnessed = false;
    for boundary in 1..=32 {
        let mut fx = SummaryFixture::new(vec![]);
        fx.begin_boot();
        let run = fx.run();
        let before = run.clock.now().unwrap();
        let label = AbsoluteBound::after(before, 10).unwrap();
        run.context
            .arm(PumpDeadline::Label, label, &mut run.clock)
            .unwrap();
        let mut clock = SummaryBoundaryClock {
            before,
            after: label.deadline_ns(),
            boundary,
            samples: 0,
            broken: false,
        };
        let result = run.context.advance_plan_summary(&mut ports(&mut clock));
        if result.is_ok() || run.context.read.is_none() {
            continue;
        }
        assert_eq!(
            result.unwrap_err(),
            RunFailure::Wait {
                key: PumpDeadline::Label,
                bound: label
            }
        );
        assert!(run.context.failure().is_none());
        let original_read_bound = run
            .context
            .deadlines
            .get(PumpDeadline::StateControl)
            .unwrap();
        assert_eq!(run.context.complete_wait(PumpDeadline::Label), Some(label));
        run.clock.advance_to(label.deadline_ns()).unwrap();
        let snapshot = fx.snapshot();
        let raw = fx.release(Some(snapshot));
        assert_eq!(
            fx.run().context.deadlines.get(PumpDeadline::StateControl),
            Some(original_read_bound)
        );
        let PlanSummaryCompletion::Boot(proof) = fx.finish() else {
            panic!("resumed accepted read must finish");
        };
        assert_eq!(proof.read().raw_bytes(), raw);
        assert_eq!(
            fx.boot
                .member
                .requests
                .iter()
                .filter(|request| matches!(
                    request,
                    s::StateControl::Read {
                        selector: s::ReadSelector::PlanCurrent {}
                    }
                ))
                .count(),
            1
        );
        witnessed = true;
        break;
    }
    assert!(
        witnessed,
        "clock sweep must find a recoverable yield after real read admission"
    );
}

#[test]
fn plan_summary_deferred_decode_custody_returns_even_when_another_pump_leg_fails() {
    let mut fx = SummaryFixture::new(vec![]);
    fx.begin_boot();
    let (mut decode, peer) = fx._decode.take().unwrap();
    let request = stage_generation(&mut decode, 5);
    let original_fd = decode.fd().as_raw_fd();
    let (admin, admin_peer) = OrganChannel::pair().unwrap();
    let mut admin = LifecyclePeer::from_channel(admin).unwrap();
    send_bytes(admin_peer.as_fd(), b"{");
    let run = fx.run();
    let mut io = SummaryReadyClock(&mut run.clock);
    let mut supplied = PumpPorts {
        lifecycle: LifecyclePorts {
            listener: None,
            peer: Some(&mut admin),
            residency: None,
            dial: None,
            leave_peer: None,
        },
        gate: None,
        decode: Some(&mut decode),
        label: None,
        io: &mut io,
    };
    assert!(matches!(
        run.context.pump_once(&mut supplied),
        Err(RunFailure::Transport {
            source: PumpSource::Admin,
            ..
        })
    ));
    let restored = supplied
        .decode
        .as_deref_mut()
        .expect("failed independent leg cannot take deferred decode custody");
    assert_eq!(restored.fd().as_raw_fd(), original_fd);
    assert!(restored.generation_not_started());
    assert!(matches!(restored.stage(request).unwrap(), Stage::Busy(_)));
    assert!(receive(peer.as_fd()).is_none());
    assert!(
        run.context.failure().is_none(),
        "caller retains the typed transport failure"
    );
}

#[path = "context_plan_summary_boot_rollback_tests.rs"]
mod boot_rollback_tests;
