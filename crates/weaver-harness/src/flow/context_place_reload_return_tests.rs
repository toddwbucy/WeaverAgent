//! Cancelled Reload returns actual capabilities and can serve a later turn.
use super::*;
use std::sync::Arc;

fn finish_staged_append(
    fx: &mut PlaceFixture,
    staged: Staged,
    expected: Vec<weaver_traits::Message>,
) {
    fx.begin_staged(staged, vec![]);
    let TokenDirective::MeasureDelta { keep, delta, .. } = fx.next_measure() else {
        panic!("returned carry must receive a fresh measure");
    };
    assert_eq!(delta, expected, "ordered carry appears exactly once");
    let input = 6;
    fx.answer(&fx.measured(keep, input));
    let PlacementCompletion::Granted(grant) = fx.take() else {
        panic!("returned carry did not receive measured placement");
    };
    fx.start_generation(grant);
    fx.sent_generation();
    fx.answer(&TokenAnswer::Generated(generated(
        keep,
        input as usize,
        0,
        "",
        Finish::Completed,
        fx.journal().2,
    )));
    assert!(matches!(
        fx.finish_generation(),
        GenerationCompletion::Generated(_)
    ));
    assert!(
        fx.run()
            .context
            .place_engine
            .as_ref()
            .unwrap()
            .staged
            .is_none()
    );
}

#[test]
fn place_reload_return_confirmed_cut_after_at_rest_stop_is_usable_in_next_turn() {
    for method in [s::CutMethod::Flush, s::CutMethod::Elide] {
        let mut fx = PlaceFixture::new();
        let units = fx.whole_span(8, "record", 80);
        let carry = fx.text(9, "carry");
        let carry_messages = carry[0].messages().to_vec();
        fx.turn("cut-original");
        fx.generation(units, 3, 1, "Read result.", Finish::Completed);
        fx.close_turn();
        let before = fx.journal();
        let mark = fx.mark(80);
        let prefix = fx.prefix(Some(mark), method);
        let mut expected = prefix.rebuild();
        expected.extend(carry_messages);
        let n = prefix.n();
        fx.begin_reload(prefix, ReloadTail::held(carry));
        assert!(matches!(
            fx.reload_packet(),
            TokenDirective::MeasureDelta { .. }
        ));
        let bound = fx.run().context.deadlines.get(PumpDeadline::Cut).unwrap();
        fx.answer(&fx.measured(n, 6));
        let cut = fx.reload_packet();
        assert!(matches!(
            cut,
            TokenDirective::Flush { .. } | TokenDirective::Elide { .. }
        ));
        fx.observe_place_command(weaver_types::LifecycleDirective::Stop {
            cause: weaver_types::Cause { uid: 0 },
        });
        assert_eq!(
            fx.run().context.deadlines.get(PumpDeadline::Cut),
            Some(bound)
        );
        assert!(
            !fx.run().context.place_turn_stopped,
            "at-rest Stop is not a run-wide latch"
        );
        fx.answer(&match method {
            s::CutMethod::Flush => TokenAnswer::Flushed {
                resident_before: before.1,
                resident_after: n,
            },
            s::CutMethod::Elide => TokenAnswer::Elided {
                resident_before: before.1,
                resident_after: n,
            },
        });
        let ReloadCompletion::Cancelled(mut account) = fx.finish_reload() else {
            panic!("Stop must retain the cancelled disposition");
        };
        let identity = account.staged_evidence().unwrap().identity.clone();
        let recorded = account.staged_evidence().unwrap().recorded.unwrap();
        assert!(!account.recorded_sequences().is_empty());
        assert!(account.raw_bytes().is_some());
        assert!(
            account.take_reload_inputs().is_none(),
            "a confirmed cut cannot return a stale prefix"
        );
        let staged = account
            .take_staged()
            .expect("confirmed cancelled cut must transfer its carry");
        assert!(Arc::ptr_eq(&identity, &staged.identity));
        assert_eq!(staged.recorded, Some(recorded));
        assert!(account.take_staged().is_none());
        assert!(
            account.take_reload_inputs().is_none(),
            "taking carry must not reopen pre-cut inputs"
        );
        assert!(receive(fx.peer.as_fd()).is_none(), "Stop grants no append");
        fx.turn("cut-next");
        finish_staged_append(&mut fx, staged, expected);
        assert!(fx.run().context.failure().is_none());
    }
}

#[test]
fn place_reload_return_queued_turn_stop_preserves_inputs_until_actual_turn_close() {
    let mut fx = PlaceFixture::new();
    let carry = fx.text(8, "carry");
    let expected = carry[0].messages().to_vec();
    fx.turn("queued-stop");
    let prefix = fx.prefix(None, s::CutMethod::Elide);
    let assembly = prefix.assembly_id();
    let before = fx.journal();
    let tail = ReloadTail::held(carry);
    let ReloadItem::Held(unit) = &tail.items[0] else {
        unreachable!()
    };
    let exact = Arc::as_ptr(unit);
    fx.begin_reload(prefix, tail);
    fx.observe_place_command(weaver_types::LifecycleDirective::Stop {
        cause: weaver_types::Cause { uid: 0 },
    });
    let ReloadCompletion::Cancelled(mut account) = fx.finish_reload() else {
        panic!("cancelled")
    };
    let inputs = account
        .take_reload_inputs()
        .expect("pre-mutation Stop returns original inputs");
    assert!(account.take_reload_inputs().is_none());
    assert_eq!(inputs.prefix.assembly_id(), assembly);
    assert!(inputs.resolved.is_empty());
    let ReloadItem::Held(unit) = &inputs.original_tail.items[0] else {
        unreachable!()
    };
    assert_eq!(Arc::as_ptr(unit), exact);
    assert_eq!(fx.journal(), before);
    assert!(receive(fx.peer.as_fd()).is_none());
    let (prefix, tail) = inputs.into_retry().unwrap();
    let run = fx.run();
    let error = run
        .context
        .begin_cut_and_reload(prefix, tail, &mut run.clock)
        .unwrap_err();
    assert_eq!(error.refusal, PlaceError::InvalidTail);
    assert!(error.failure.is_none());
    assert_eq!(error.input.0.assembly_id(), assembly);
    fx.close_turn();
    let staged = fx.staged(error.input.0, error.input.1, 3);
    fx.turn("queued-next");
    finish_staged_append(&mut fx, staged, expected);
}

#[test]
fn place_reload_return_filed_pending_tail_retries_without_refiling_or_reclassification() {
    let mut fx = PlaceFixture::new();
    let original = fx.whole_span(8, "record", 80);
    let held_first = fx.text(9, "first");
    let held_last = fx.text(10, "last");
    let first_messages = held_first[0].messages().to_vec();
    let last_messages = held_last[0].messages().to_vec();
    fx.turn("pending-return");
    fx.hold_files = true;
    let mut generation = fx.generation(original, 3, 1, "Read pending.", Finish::Completed);
    let mut output = generation.take_output().unwrap();
    let source = output.source().block_id.clone();
    let lease = output.take_receipt_lease().unwrap();
    let pending = fx
        .run()
        .context
        .hold_reload_text(lease, source.clone())
        .unwrap();
    let mut tail = ReloadTail::held(held_first);
    tail.push_pending(pending);
    tail.items.extend(ReloadTail::held(held_last).items);
    fx.close_turn();
    let mark = fx.mark(80);
    let prefix = fx.prefix(Some(mark), s::CutMethod::Flush);
    let rebuild = prefix.rebuild();
    let before = fx.journal();
    fx.begin_reload(prefix, tail);
    fx.hold_files = false;
    let TokenDirective::MeasureDelta { keep, delta, .. } = fx.reload_packet() else {
        panic!("measure")
    };
    let checks = fx.records(Kind::Ste100Result).len();
    let files = fx.summary.files.len();
    assert_eq!(
        fx.summary
            .files
            .iter()
            .filter(|file| file.block_id == source)
            .count(),
        1
    );
    let prefix_len = rebuild.len();
    let first_len = first_messages.len();
    assert_eq!(&delta[..prefix_len], rebuild.as_slice());
    assert_eq!(
        &delta[prefix_len..prefix_len + first_len],
        first_messages.as_slice()
    );
    assert_eq!(
        delta.len(),
        prefix_len + first_len + 1 + last_messages.len()
    );
    assert_eq!(
        &delta[delta.len() - last_messages.len()..],
        last_messages.as_slice()
    );
    assert_eq!(
        delta[delta.len() - 2].content,
        vec![ContentBlock::Text {
            text: "Read pending.".into()
        }]
    );
    let wanted = delta.clone();
    fx.answer(&fx.measured(keep, u64::MAX));
    let ReloadCompletion::Refused(mut account) = fx.finish_reload() else {
        panic!("fit refusal")
    };
    assert_eq!(fx.journal(), before);
    let inputs = account
        .take_reload_inputs()
        .expect("no-mutation refusal returns real tail");
    assert!(inputs.rejected.is_none());
    assert_eq!(inputs.resolved.len(), 3);
    let exact_units: Vec<_> = inputs.resolved.iter().map(Arc::as_ptr).collect();
    let ReloadItem::Pending(original_pending) = &inputs.original_tail.items[1] else {
        panic!("original retained")
    };
    assert_eq!(original_pending.source, source);
    assert_eq!(
        fx.run()
            .context
            .flow
            .source_receipt_status(&original_pending.lease, &source)
            .unwrap(),
        crate::flow::ledger::LeaseSlotStatus::Taken
    );
    let (prefix, tail) = inputs.into_retry().unwrap();
    assert_eq!(tail.filed_sources.len(), 1);
    assert_eq!(tail.filed_sources[0].0, 1);
    assert_eq!(tail.filed_sources[0].1.source, source);
    for (item, exact) in tail.items.iter().zip(exact_units) {
        let ReloadItem::Held(unit) = item else {
            panic!("resolved input stays held")
        };
        assert_eq!(Arc::as_ptr(unit), exact);
    }
    let run = fx.run();
    assert!(run.context.place_close().is_some());
    run.context.place_response_sent(&mut run.clock).unwrap();
    let staged = fx.staged(prefix, tail, 6);
    assert_eq!(fx.records(Kind::Ste100Result).len(), checks);
    assert_eq!(fx.summary.files.len(), files);
    fx.turn("pending-retry-next");
    finish_staged_append(&mut fx, staged, wanted);
    assert_eq!(fx.records(Kind::Ste100Result).len(), checks);
    assert_eq!(fx.summary.files.len(), files);
}

#[test]
fn place_reload_return_cancelled_staged_leave_keeps_exact_grant_but_refuses_next_append() {
    for join in [false, true] {
        let mut fx = PlaceFixture::new();
        let carry = fx.text(8, "carry");
        let prefix = fx.prefix(None, s::CutMethod::Elide);
        let n = prefix.n();
        fx.begin_reload(prefix, ReloadTail::held(carry));
        fx.reload_packet();
        fx.answer(&fx.measured(n, 3));
        // Settle the actual empty-suffix cut, return its cancelled carry, then
        // establish a real turn/wait before Leave so no other guard explains refusal.
        for _ in 0..128 {
            if fx.advance_reload().unwrap() == ReloadProgress::Complete {
                break;
            }
        }
        let run = fx.run();
        run.context.cancel_reload(&mut run.clock).unwrap();
        let ReloadCompletion::Cancelled(mut account) = fx.finish_reload() else {
            panic!("cancelled")
        };
        let staged = account.take_staged().unwrap();
        fx.turn("staged-before-leave");
        fx.decision();
        fx.observe_place_command(if join {
            weaver_types::LifecycleDirective::JoinLeave {
                cause: weaver_types::Cause { uid: 0 },
            }
        } else {
            weaver_types::LifecycleDirective::Leave {
                cause: weaver_types::Cause { uid: 0 },
                forced: true,
                rollback: true,
            }
        });
        let identity = staged.identity.clone();
        let run = &mut fx.summary.boot.member.run;
        let error = run
            .context
            .begin_staged_place(
                staged,
                vec![],
                s::PlacementContext {
                    label: "leave refusal".into(),
                    instruction: None,
                },
                &mut fx.decode,
                &mut run.clock,
            )
            .unwrap_err();
        assert!(Arc::ptr_eq(&identity, &error.staged.identity));
        assert_eq!(error.refusal, PlaceError::WrongPhase);
        assert!(error.failure.is_none());
        assert!(receive(fx.peer.as_fd()).is_none());
        assert_eq!(fx.journal().6, 0);
    }
}

#[test]
fn place_reload_return_cancel_before_filing_keeps_original_pending_source_lease() {
    let mut fx = PlaceFixture::new();
    let units = fx.whole_span(8, "record", 80);
    fx.turn("pending-cancel-source");
    fx.hold_files = true;
    let mut generated = fx.generation(units, 3, 1, "Read pending.", Finish::Completed);
    let mut output = generated.take_output().unwrap();
    let source = output.source().block_id.clone();
    let lease = output.take_receipt_lease().unwrap();
    let pending = fx
        .run()
        .context
        .hold_reload_text(lease, source.clone())
        .unwrap();
    fx.close_turn();
    let mark = fx.mark(80);
    let prefix = fx.prefix(Some(mark), s::CutMethod::Flush);
    let mut tail = ReloadTail::held(vec![]);
    tail.push_pending(pending);
    fx.begin_reload(prefix, tail);
    fx.observe_place_command(weaver_types::LifecycleDirective::Stop {
        cause: weaver_types::Cause { uid: 0 },
    });
    let ReloadCompletion::Cancelled(mut account) = fx.finish_reload() else {
        panic!("cancelled")
    };
    let inputs = account.take_reload_inputs().unwrap();
    assert!(inputs.resolved.is_empty());
    assert!(inputs.original_tail.filed_sources.is_empty());
    let ReloadItem::Pending(pending) = &inputs.original_tail.items[0] else {
        panic!("original pending")
    };
    assert_eq!(pending.source, source);
    assert!(
        fx.run()
            .context
            .flow
            .ledger()
            .owned_pending_text(&pending.lease, &source)
    );
    assert!(receive(fx.peer.as_fd()).is_none());
    let (prefix, tail) = inputs.into_retry().unwrap();
    let checks = fx.records(Kind::Ste100Result).len();
    fx.hold_files = false;
    fx.begin_reload(prefix, tail);
    let TokenDirective::MeasureDelta { keep, delta, .. } = fx.reload_packet() else {
        panic!("measure")
    };
    assert_eq!(
        delta.last().unwrap().content,
        vec![ContentBlock::Text {
            text: "Read pending.".into()
        }]
    );
    fx.answer(&fx.measured(keep, 6));
    let before = fx.journal().1;
    assert_eq!(fx.reload_packet(), TokenDirective::Flush { keep });
    fx.answer(&TokenAnswer::Flushed {
        resident_before: before,
        resident_after: keep,
    });
    let ReloadCompletion::Staged(staged) = fx.finish_reload() else {
        panic!("staged")
    };
    assert_eq!(fx.records(Kind::Ste100Result).len(), checks);
    assert_eq!(
        fx.summary
            .files
            .iter()
            .filter(|f| f.block_id == source)
            .count(),
        1
    );
    fx.turn("pending-cancel-next");
    finish_staged_append(&mut fx, staged, delta);
}
