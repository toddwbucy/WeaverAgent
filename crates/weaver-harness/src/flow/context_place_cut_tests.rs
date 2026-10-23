//! Physical cuts and the private nonresident carry, through real reservations.
use super::*;

impl PlaceFixture {
    pub(super) fn mark(&self, local_id: u64) -> s::CutReference {
        let local_id = local_id.to_string();
        let record = self
            .records(Kind::StateMarkPosition)
            .into_iter()
            .rev()
            .find(|r| r["payload"]["span"]["local_id"].as_str() == Some(local_id.as_str()))
            .expect("only an actually recorded placed or boot occurrence can name the cut");
        s::CutReference {
            span: serde_json::from_value(record["payload"]["span"].clone()).unwrap(),
            occurrence_id: serde_json::from_value(record["payload"]["id"].clone()).unwrap(),
        }
    }
    pub(super) fn prefix(
        &mut self,
        cut: Option<s::CutReference>,
        method: s::CutMethod,
    ) -> PreparedPrefix {
        let run = self.run();
        run.context
            .prepare_prefix(cut, method, &mut run.clock)
            .unwrap()
    }
    pub(super) fn begin_reload(&mut self, prefix: PreparedPrefix, tail: ReloadTail) {
        let run = self.run();
        run.context
            .begin_cut_and_reload(prefix, tail, &mut run.clock)
            .unwrap();
    }
    pub(super) fn advance_reload(&mut self) -> Result<ReloadProgress, RunFailure> {
        let run = &mut self.summary.boot.member.run;
        let mut io = ReadyClock {
            clock: &mut run.clock,
            fd: self.decode.fd().as_raw_fd(),
        };
        let result = run.context.advance_cut_and_reload(&mut PumpPorts {
            lifecycle: LifecyclePorts::default(),
            gate: None,
            decode: Some(&mut self.decode),
            label: None,
            io: &mut io,
        });
        if result.is_ok() {
            self.classifier(false);
            if !self.hold_files {
                self.summary.file_reply();
            }
        }
        result
    }
    pub(super) fn reload_packet(&mut self) -> TokenDirective {
        for _ in 0..256 {
            if let Some(raw) = receive(self.peer.as_fd()) {
                return serde_json::from_slice(&raw).unwrap();
            }
            self.advance_reload().unwrap();
        }
        panic!("cut owner did not send its expected request");
    }
    pub(super) fn finish_reload(&mut self) -> ReloadCompletion {
        for _ in 0..256 {
            self.advance_reload().unwrap();
            let run = self.run();
            if let Some(done) = run.context.take_reload(&mut run.clock).unwrap() {
                return done;
            }
        }
        panic!("cut owner did not settle");
    }
    pub(super) fn staged(
        &mut self,
        prefix: PreparedPrefix,
        tail: ReloadTail,
        input: u64,
    ) -> Staged {
        let method = prefix.method.unwrap();
        let before = prefix.resident_before();
        let n = prefix.n();
        self.begin_reload(prefix, tail);
        let TokenDirective::MeasureDelta { keep, .. } = self.reload_packet() else {
            panic!("cut must measure before mutation");
        };
        assert_eq!(keep, n);
        self.answer(&self.measured(keep, input));
        match method {
            s::CutMethod::Flush => {
                assert_eq!(self.reload_packet(), TokenDirective::Flush { keep: n });
                self.answer(&TokenAnswer::Flushed {
                    resident_before: before,
                    resident_after: n,
                });
            }
            s::CutMethod::Elide if before != n => {
                assert_eq!(
                    self.reload_packet(),
                    TokenDirective::Elide {
                        from: n,
                        to: before
                    }
                );
                self.answer(&TokenAnswer::Elided {
                    resident_before: before,
                    resident_after: n,
                });
            }
            s::CutMethod::Elide => {}
        }
        let ReloadCompletion::Staged(staged) = self.finish_reload() else {
            panic!("valid cut did not preserve staged carry");
        };
        assert_eq!(staged.resident_after(), n);
        assert!(receive(self.peer.as_fd()).is_none());
        staged
    }
    pub(super) fn begin_staged(&mut self, staged: Staged, additional: Vec<SelectedUnit>) {
        self.decision();
        let run = &mut self.summary.boot.member.run;
        run.context
            .begin_staged_place(
                staged,
                additional,
                s::PlacementContext {
                    label: "actual staged continuation".into(),
                    instruction: None,
                },
                &mut self.decode,
                &mut run.clock,
            )
            .unwrap();
    }
}

#[test]
fn place_flush_and_elide_cut_at_actual_mark_keep_conservative_n_and_rebuild_ids_once() {
    for method in [s::CutMethod::Flush, s::CutMethod::Elide] {
        let mut fx = PlaceFixture::new();
        let first = fx.whole_span(8, "caf\u{e9}", 80);
        let first_messages = first[0].messages().to_vec();
        fx.turn("original-prefix");
        fx.generation(first, 3, 2, "Read first.", Finish::Completed);
        let mark = fx.mark(80);
        let first_record = fx.records(Kind::StatePlacement).pop().unwrap();
        let first_id = first_record["payload"]["message_ids"][0].clone();
        let later = fx.whole_span(9, "later", 81);
        fx.generation(later, 3, 2, "Read later.", Finish::Completed);
        let removed = fx.mark(81);
        let carry = fx.text(10, "carry");
        let carry_messages = carry[0].messages().to_vec();
        let added = fx.text(11, "added");
        let added_messages = added[0].messages().to_vec();
        fx.close_turn();
        let before = fx.journal();
        let prefix = fx.prefix(Some(mark.clone()), method);
        assert_eq!(
            prefix.n(),
            before.0,
            "mark before first assistant cannot use that exchange's terminal"
        );
        assert_eq!(prefix.rebuild(), first_messages);
        let staged = fx.staged(prefix, ReloadTail::held(carry), 6);
        assert_eq!(fx.journal().1, before.0);
        assert_eq!(
            fx.journal().6,
            1,
            "carry is not resident until a complete append"
        );
        assert!(fx.journal().7.iter().any(|(ordinal, span, _)| *ordinal
            == mark.occurrence_id.ordinal.0
            && span == &mark.span));
        assert!(
            !fx.journal()
                .7
                .iter()
                .any(|(ordinal, _, _)| *ordinal == removed.occurrence_id.ordinal.0)
        );
        let cut = fx.records(Kind::StatePlacement).pop().unwrap();
        assert_eq!(cut["payload"]["phase"], "cut");
        assert_eq!(cut["payload"]["message_ids"], serde_json::json!([]));
        assert_eq!(cut["payload"]["unit_map"], serde_json::json!([]));
        assert!(cut["payload"]["assistant_message_id"].is_null());
        assert_eq!(
            fx.records(if method == s::CutMethod::Flush {
                Kind::Flush
            } else {
                Kind::Elision
            })
            .len(),
            1
        );
        fx.turn("staged-continuation");
        fx.begin_staged(staged, added);
        let TokenDirective::MeasureDelta {
            keep,
            delta,
            tool_schemas,
        } = fx.next_measure()
        else {
            unreachable!()
        };
        let expected = [first_messages, carry_messages, added_messages].concat();
        assert_eq!(keep, before.0);
        assert_eq!(delta, expected);
        assert_eq!(tool_schemas, fx.schemas());
        assert_eq!(fx.journal().6, 1);
        fx.answer(&fx.measured(keep, 9));
        let PlacementCompletion::Granted(grant) = fx.take() else {
            panic!("staged continuation refused");
        };
        let append = fx.records(Kind::StatePlacement).pop().unwrap();
        assert_eq!(append["payload"]["message_ids"][0], first_id);
        assert_eq!(append["payload"]["recomputed_messages"], "1");
        assert_eq!(append["payload"]["unit_map"][0]["message_start"], "1");
        assert_eq!(append["payload"]["unit_map"][1]["message_start"], "2");
        fx.start_generation(grant);
        let TokenDirective::AppendAndGenerate { delta, .. } = fx.sent_generation() else {
            unreachable!()
        };
        assert_eq!(delta, expected);
        fx.answer(&TokenAnswer::Generated(generated(
            keep,
            9,
            1,
            "Read final.",
            Finish::Completed,
            before.2,
        )));
        assert!(matches!(
            fx.finish_generation(),
            GenerationCompletion::Generated(_)
        ));
        assert_eq!(fx.journal().6, 4);
        fx.close_turn();
        let run = fx.run();
        assert!(
            run.context
                .prepare_prefix(Some(removed), method, &mut run.clock)
                .is_err(),
            "removed mark never revives"
        );
        let refused = fx.finish_notice();
        assert_eq!(refused.code, s::PlacementCode::InvalidCut);
        let run = fx.run();
        run.context.place_response_sent(&mut run.clock).unwrap();
        let kept = run
            .context
            .prepare_prefix(Some(mark), method, &mut run.clock)
            .unwrap();
        assert_eq!(
            kept.n(),
            before.0,
            "removed earlier boundaries are never reused"
        );
    }
}

#[test]
fn place_empty_elide_stages_without_elision_and_exclusive_carry_blocks_another_cut() {
    let mut fx = PlaceFixture::new();
    let units = fx.text(8, "record");
    fx.turn("empty-cut-source");
    fx.generation(units, 3, 1, "Read result.", Finish::Completed);
    fx.close_turn();
    let before = fx.journal();
    let prefix = fx.prefix(None, s::CutMethod::Elide);
    assert_eq!(prefix.n(), before.1);
    assert!(prefix.rebuild().is_empty());
    let staged = fx.staged(prefix, ReloadTail::held(vec![]), 0);
    assert_eq!(fx.records(Kind::Elision).len(), 0);
    assert_eq!(fx.journal().1, before.1);
    assert_eq!(fx.journal().6, before.6);
    let run = fx.run();
    assert_eq!(
        run.context
            .prepare_prefix(None, s::CutMethod::Flush, &mut run.clock)
            .unwrap_err(),
        PlaceError::Busy
    );
    fx.turn("empty-carry-next");
    fx.begin_staged(staged, vec![]);
    let TokenDirective::MeasureDelta { keep, delta, .. } = fx.next_measure() else {
        unreachable!()
    };
    assert_eq!(keep, before.1);
    assert!(
        delta.is_empty(),
        "empty carry is valid and still remeasured"
    );
    fx.answer(&fx.measured(keep, 0));
    let PlacementCompletion::Granted(grant) = fx.take() else {
        panic!("empty carry refused");
    };
    fx.start_generation(grant);
    fx.sent_generation();
    fx.answer(&TokenAnswer::Generated(generated(
        keep,
        0,
        0,
        "",
        Finish::Completed,
        before.2,
    )));
    assert!(matches!(
        fx.finish_generation(),
        GenerationCompletion::Generated(_)
    ));
    assert_eq!(fx.journal().6, before.6 + 1);
}

#[test]
fn place_cut_queues_while_turn_open_and_starts_only_one_bound_after_actual_close() {
    let mut fx = PlaceFixture::new();
    let units = fx.whole_span(8, "record", 80);
    fx.turn("open-cut-turn");
    fx.generation(units, 3, 1, "Read result.", Finish::Completed);
    let mark = fx.mark(80);
    let prefix = fx.prefix(Some(mark), s::CutMethod::Flush);
    fx.begin_reload(prefix, ReloadTail::held(vec![]));
    for _ in 0..8 {
        assert_ne!(fx.advance_reload().unwrap(), ReloadProgress::Complete);
        assert!(fx.run().context.deadlines.get(PumpDeadline::Cut).is_none());
        assert!(receive(fx.peer.as_fd()).is_none());
    }
    fx.close_turn();
    let TokenDirective::MeasureDelta { keep, .. } = fx.reload_packet() else {
        unreachable!()
    };
    let bound = fx.run().context.deadlines.get(PumpDeadline::Cut).unwrap();
    for _ in 0..8 {
        fx.advance_reload().unwrap();
        assert_eq!(
            fx.run().context.deadlines.get(PumpDeadline::Cut),
            Some(bound)
        );
        assert!(fx.run().context.complete_wait(PumpDeadline::Cut).is_none());
    }
    fx.answer(&fx.measured(keep, 3));
    assert_eq!(fx.reload_packet(), TokenDirective::Flush { keep });
    let before = fx.journal().1;
    fx.answer(&TokenAnswer::Flushed {
        resident_before: before,
        resident_after: keep,
    });
    assert!(matches!(fx.finish_reload(), ReloadCompletion::Staged(_)));
    assert!(fx.run().context.deadlines.get(PumpDeadline::Cut).is_none());
}

#[test]
fn place_wrong_cut_counts_never_install_a_fictional_prefix_or_staged_capability() {
    for (method, bad_before) in [
        (s::CutMethod::Flush, true),
        (s::CutMethod::Flush, false),
        (s::CutMethod::Elide, true),
        (s::CutMethod::Elide, false),
    ] {
        let mut fx = PlaceFixture::new();
        let units = fx.whole_span(8, "record", 80);
        fx.turn("bad-cut");
        fx.generation(units, 3, 1, "Read result.", Finish::Completed);
        fx.close_turn();
        let before = fx.journal();
        let mark = fx.mark(80);
        let prefix = fx.prefix(Some(mark), method);
        let n = prefix.n();
        fx.begin_reload(prefix, ReloadTail::held(vec![]));
        assert!(matches!(
            fx.reload_packet(),
            TokenDirective::MeasureDelta { .. }
        ));
        fx.answer(&fx.measured(n, 3));
        fx.reload_packet();
        let resident_before = before.1 + u64::from(bad_before);
        let resident_after = n + u64::from(!bad_before);
        fx.answer(&match method {
            s::CutMethod::Flush => TokenAnswer::Flushed {
                resident_before,
                resident_after,
            },
            s::CutMethod::Elide => TokenAnswer::Elided {
                resident_before,
                resident_after,
            },
        });
        let mut failure = None;
        for _ in 0..128 {
            if let Err(found) = fx.advance_reload() {
                failure = Some(found);
                break;
            }
        }
        assert!(failure.is_some());
        assert_eq!(fx.journal().1, before.1);
        assert_eq!(fx.journal().6, before.6);
        assert_eq!(
            fx.records(Kind::StatePlacement).len(),
            2,
            "no fabricated cut placement"
        );
        assert!(receive(fx.peer.as_fd()).is_none());
    }
}

#[test]
fn place_reload_retains_real_pending_text_lease_until_its_original_filed_receipt() {
    let mut fx = PlaceFixture::new();
    let units = fx.whole_span(8, "record", 80);
    fx.turn("pending-text-source");
    fx.hold_files = true;
    let mut generation = fx.generation(units, 3, 1, "Read pending.", Finish::Completed);
    let mut checked = generation
        .take_output()
        .expect("actual assistant source proof");
    let source = checked.source().block_id.clone();
    let lease = checked
        .take_receipt_lease()
        .expect("actual selected-output receipt subscription");
    let run = fx.run();
    let pending = run.context.hold_reload_text(lease, source.clone()).unwrap();
    let mark = fx.mark(80);
    fx.close_turn();
    let prefix = fx.prefix(Some(mark), s::CutMethod::Flush);
    let mut tail = ReloadTail::held(vec![]);
    tail.push_pending(pending);
    let checks = fx.records(Kind::Ste100Result).len();
    fx.begin_reload(prefix, tail);
    for _ in 0..8 {
        assert_ne!(fx.advance_reload().unwrap(), ReloadProgress::Complete);
        assert!(
            receive(fx.peer.as_fd()).is_none(),
            "pending original filing blocks even the cut measurement"
        );
        assert!(
            fx.summary.files.is_empty(),
            "member has not received a fabricated duplicate filing"
        );
    }
    fx.hold_files = false;
    let TokenDirective::MeasureDelta { keep, delta, .. } = fx.reload_packet() else {
        panic!("released original filing must permit measure");
    };
    assert_eq!(
        delta.last().unwrap().content,
        vec![ContentBlock::Text {
            text: "Read pending.".into()
        }]
    );
    assert_eq!(
        fx.records(Kind::Ste100Result).len(),
        checks,
        "reloading a checked live source never reclassifies it"
    );
    assert_eq!(
        fx.summary
            .files
            .iter()
            .filter(|f| f.block_id == source)
            .count(),
        1
    );
    fx.answer(&fx.measured(keep, 6));
    assert_eq!(fx.reload_packet(), TokenDirective::Flush { keep });
    let before = fx.journal().1;
    fx.answer(&TokenAnswer::Flushed {
        resident_before: before,
        resident_after: keep,
    });
    assert!(matches!(fx.finish_reload(), ReloadCompletion::Staged(_)));
}

#[test]
fn place_reload_cancel_after_real_cut_preserves_physical_truth_and_staged_carry() {
    for before_send in [true, false] {
        let mut fx = PlaceFixture::new();
        let units = fx.whole_span(8, "record", 80);
        fx.turn("cut-cancellation");
        fx.generation(units, 3, 1, "Read result.", Finish::Completed);
        fx.close_turn();
        let mark = fx.mark(80);
        let prefix = fx.prefix(Some(mark), s::CutMethod::Flush);
        let before = fx.journal();
        let n = prefix.n();
        fx.begin_reload(prefix, ReloadTail::held(vec![]));
        if before_send {
            let run = fx.run();
            run.context.cancel_reload(&mut run.clock).unwrap();
            assert!(matches!(fx.finish_reload(), ReloadCompletion::Cancelled(_)));
            assert!(receive(fx.peer.as_fd()).is_none());
            assert_eq!(fx.journal().1, before.1);
            assert_eq!(fx.records(Kind::Flush).len(), 0);
        } else {
            assert!(matches!(
                fx.reload_packet(),
                TokenDirective::MeasureDelta { .. }
            ));
            fx.answer(&fx.measured(n, 3));
            assert_eq!(fx.reload_packet(), TokenDirective::Flush { keep: n });
            let run = fx.run();
            run.context.cancel_reload(&mut run.clock).unwrap();
            for _ in 0..4 {
                fx.advance_reload().unwrap();
                let run = fx.run();
                assert!(run.context.take_reload(&mut run.clock).unwrap().is_none());
            }
            fx.answer(&TokenAnswer::Flushed {
                resident_before: before.1,
                resident_after: n,
            });
            let ReloadCompletion::Cancelled(account) = fx.finish_reload() else {
                panic!("cancelled cut must retain an account");
            };
            let staged = account
                .staged_evidence()
                .expect("actual physical cut keeps its nonresident carry as evidence");
            assert_eq!(staged.resident_after(), n);
            assert_eq!(fx.journal().1, n);
            assert_eq!(fx.records(Kind::Flush).len(), 1);
            assert_eq!(fx.records(Kind::StatePlacement).len(), 3);
            assert!(
                receive(fx.peer.as_fd()).is_none(),
                "cancellation grants no generation"
            );
        }
    }
}

#[test]
fn place_staged_added_fit_refusal_returns_same_carry_and_remeasures_retry_once() {
    let mut fx = PlaceFixture::new();
    let carry = fx.text(8, "carry");
    let carry_messages = carry[0].messages().to_vec();
    let added = fx.text(9, "added");
    let added_messages = added[0].messages().to_vec();
    let prefix = fx.prefix(None, s::CutMethod::Elide);
    let staged = fx.staged(prefix, ReloadTail::held(carry), 3);
    let assembly = staged.assembly_id();
    let resident = staged.resident_after();
    fx.turn("staged-too-large");
    fx.begin_staged(staged, added);
    let TokenDirective::MeasureDelta { keep, delta, .. } = fx.next_measure() else {
        unreachable!()
    };
    assert_eq!(
        delta,
        [carry_messages.clone(), added_messages.clone()].concat()
    );
    fx.answer(&fx.measured(keep, u64::MAX));
    let PlacementCompletion::Refused(mut refused) = fx.take() else {
        panic!("oversized added input granted");
    };
    assert_eq!(refused.code, s::PlacementCode::TailOverCapacity);
    let account = refused.account_mut().unwrap();
    let staged = account
        .take_staged()
        .expect("failed added-input fit retains exclusive carry");
    let added = account
        .take_unplaced_selection()
        .expect("failed candidate retains exact added material");
    assert_eq!(staged.assembly_id(), assembly);
    assert_eq!(staged.resident_after(), resident);
    assert_eq!(added[0].messages(), added_messages);
    assert_eq!(fx.journal().6, 0);
    assert!(receive(fx.peer.as_fd()).is_none());
    let run = fx.run();
    assert!(run.context.place_close().is_some());
    run.context.place_response_sent(&mut run.clock).unwrap();
    fx.turn("staged-fit-retry");
    fx.begin_staged(staged, added);
    let TokenDirective::MeasureDelta { keep, delta, .. } = fx.next_measure() else {
        unreachable!()
    };
    assert_eq!(delta, [carry_messages, added_messages].concat());
    fx.answer(&fx.measured(keep, 6));
    let PlacementCompletion::Granted(grant) = fx.take() else {
        panic!("valid explicit retry refused");
    };
    fx.start_generation(grant);
    fx.sent_generation();
    fx.answer(&TokenAnswer::Generated(generated(
        keep,
        6,
        0,
        "",
        Finish::Completed,
        fx.journal().2,
    )));
    assert!(matches!(
        fx.finish_generation(),
        GenerationCompletion::Generated(_)
    ));
    assert_eq!(
        fx.journal().6,
        3,
        "carry and added material each occur once plus the assistant"
    );
}

#[test]
fn place_pending_output_lease_preserves_preclassification_acceptance_and_correct_rejection() {
    for (reject, queue_after_terminal) in
        [(false, false), (true, false), (false, true), (true, true)]
    {
        let mut fx = PlaceFixture::new();
        let units = fx.whole_span(8, "record", 80);
        fx.turn("pending-unchecked");
        fx.hold_files = true;
        let boot = fx
            .records(Kind::StateMarkPosition)
            .into_iter()
            .next()
            .unwrap();
        let cut = s::CutReference {
            span: serde_json::from_value(boot["payload"]["span"].clone()).unwrap(),
            occurrence_id: serde_json::from_value(boot["payload"]["id"].clone()).unwrap(),
        };
        let queued_prefix = fx.prefix(Some(cut), s::CutMethod::Flush);
        let before = fx.journal();
        let checks = fx.records(Kind::Ste100Result).len();
        let grant = fx.grant(units, 3);
        fx.start_generation(grant);
        fx.sent_generation();
        fx.answer(&TokenAnswer::Generated(generated(
            before.1,
            3,
            1,
            "Read pending.",
            Finish::Completed,
            before.2,
        )));
        let mut pending = None;
        for _ in 0..128 {
            let run = &mut fx.summary.boot.member.run;
            let mut io = ReadyClock {
                clock: &mut run.clock,
                fd: fx.decode.fd().as_raw_fd(),
            };
            run.context
                .advance_generation(&mut PumpPorts {
                    lifecycle: LifecyclePorts::default(),
                    gate: None,
                    decode: Some(&mut fx.decode),
                    label: None,
                    io: &mut io,
                })
                .unwrap();
            if let Ok(lease) = run.context.hold_pending_output_reload_text(0) {
                pending = Some(lease);
                break;
            }
        }
        let pending = pending.expect(
            "actual Recorded output must expose its owned original Text before classification",
        );
        assert_eq!(fx.records(Kind::Ste100Result).len(), checks);
        assert!(
            fx.run().context.hold_pending_output_reload_text(0).is_err(),
            "one subscription transfers only once"
        );
        let mut tail = ReloadTail::held(vec![]);
        tail.push_pending(pending);
        let mut queued = Some((queued_prefix, tail));
        if !queue_after_terminal {
            let (prefix, tail) = queued.take().unwrap();
            fx.begin_reload(prefix, tail);
            assert!(
                fx.run().context.deadlines.get(PumpDeadline::Cut).is_none(),
                "queued cut owns no wait before actual turn close"
            );
            assert!(receive(fx.peer.as_fd()).is_none());
        }
        let mut completed = None;
        for _ in 0..128 {
            let run = &mut fx.summary.boot.member.run;
            let mut io = ReadyClock {
                clock: &mut run.clock,
                fd: fx.decode.fd().as_raw_fd(),
            };
            run.context
                .advance_generation(&mut PumpPorts {
                    lifecycle: LifecyclePorts::default(),
                    gate: None,
                    decode: Some(&mut fx.decode),
                    label: None,
                    io: &mut io,
                })
                .unwrap();
            fx.classifier(reject);
            let run = fx.run();
            completed = run.context.take_placed_generation(&mut run.clock).unwrap();
            if completed.is_some() {
                break;
            }
        }
        assert!(matches!(
            completed,
            Some(GenerationCompletion::Generated(_))
        ));
        assert_eq!(fx.records(Kind::Ste100Result).len(), checks + 1);
        if queue_after_terminal {
            fx.hold_files = false;
            fx.drain_files();
            let (prefix, tail) = queued.take().unwrap();
            let ReloadItem::Pending(pending) = &tail.items[0] else {
                unreachable!()
            };
            assert_eq!(
                fx.run()
                    .context
                    .flow
                    .ledger()
                    .source_lease_status(&pending.lease, &pending.source)
                    .unwrap(),
                crate::flow::ledger::LeaseSlotStatus::Ready,
                "queue starts only after the actual terminal disposition"
            );
            assert!(
                fx.run().context.flow.ledger().facts().next().is_none(),
                "retired source must be owned by the immutable receipt"
            );
            fx.begin_reload(prefix, tail);
            assert!(fx.run().context.deadlines.get(PumpDeadline::Cut).is_none());
        }
        fx.close_turn();
        if reject {
            let ReloadCompletion::Refused(mut account) = fx.finish_reload() else {
                panic!("correctly rejected pending Text must refuse reload");
            };
            assert_eq!(
                account.reload_refusal().map(|(code, _)| code),
                Some(ReloadCode::NonSte100)
            );
            assert!(fx.run().context.failure().is_none());
            assert!(receive(fx.peer.as_fd()).is_none());
            assert_eq!(fx.records(Kind::Flush).len(), 0);
            assert!(fx.summary.files.is_empty());
            let inputs = account
                .take_reload_inputs()
                .expect("rejection returns original input custody");
            assert!(matches!(
                inputs.rejected.as_ref(),
                Some(crate::flow::ledger::SourceDisposition::CorrectlyRejected(_))
            ));
            assert!(
                inputs.resolved.is_empty(),
                "rejected Text never becomes a held selected unit"
            );
            assert!(matches!(
                &inputs.original_tail.items[0],
                ReloadItem::Pending(_)
            ));
            let rejected = inputs
                .into_retry()
                .expect_err("retry cannot silently accept or drop rejected Text");
            assert!(rejected.rejected.is_some());
            assert!(account.take_reload_inputs().is_none());
        } else {
            if !queue_after_terminal {
                for _ in 0..4 {
                    fx.advance_reload().unwrap();
                    assert!(receive(fx.peer.as_fd()).is_none());
                }
                fx.hold_files = false;
            }
            let TokenDirective::MeasureDelta { keep, delta, .. } = fx.reload_packet() else {
                panic!("accepted original Text must be measured after Filed");
            };
            assert_eq!(
                delta.last().unwrap().content,
                vec![ContentBlock::Text {
                    text: "Read pending.".into()
                }]
            );
            assert_eq!(fx.summary.files.len(), 1);
            fx.answer(&fx.measured(keep, 6));
            assert_eq!(fx.reload_packet(), TokenDirective::Flush { keep });
            fx.answer(&TokenAnswer::Flushed {
                resident_before: fx.journal().1,
                resident_after: keep,
            });
            assert!(matches!(fx.finish_reload(), ReloadCompletion::Staged(_)));
        }
        assert_eq!(
            fx.records(Kind::Ste100Result).len(),
            checks + 1,
            "reload never reclassifies the original source"
        );
    }
}

#[test]
fn place_queued_unmarked_prefix_keeps_original_endpoint_instead_of_later_current_end() {
    let mut fx = PlaceFixture::new();
    let one = fx.text(8, "first");
    let two = fx.text(9, "second");
    fx.turn("queued-exact-end");
    fx.generation(one, 3, 0, "", Finish::Completed);
    let earlier_resident = fx.journal().1;
    let earlier_messages = fx.journal().6;
    let prefix = fx.prefix(None, s::CutMethod::Flush);
    let grant = fx.grant(two, 3);
    fx.start_generation(grant);
    fx.begin_reload(prefix, ReloadTail::held(vec![]));
    fx.sent_generation();
    fx.answer(&TokenAnswer::Generated(generated(
        earlier_resident,
        3,
        0,
        "",
        Finish::Completed,
        fx.journal().2,
    )));
    assert!(matches!(
        fx.finish_generation(),
        GenerationCompletion::Generated(_)
    ));
    assert!(fx.journal().1 > earlier_resident);
    fx.close_turn();
    let before = fx.journal().1;
    let TokenDirective::MeasureDelta { keep, delta, .. } = fx.reload_packet() else {
        panic!("queued exact end must measure");
    };
    assert_eq!(keep, earlier_resident);
    assert!(delta.is_empty());
    fx.answer(&fx.measured(keep, 0));
    assert_eq!(fx.reload_packet(), TokenDirective::Flush { keep });
    fx.answer(&TokenAnswer::Flushed {
        resident_before: before,
        resident_after: keep,
    });
    assert!(matches!(fx.finish_reload(), ReloadCompletion::Staged(_)));
    assert_eq!(fx.journal().1, earlier_resident);
    assert_eq!(fx.journal().6, earlier_messages);
}

#[path = "context_place_reload_return_tests.rs"]
mod return_review_tests;
