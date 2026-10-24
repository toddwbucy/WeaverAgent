//! Canonical held material, original pair identity, UTF-8 ranges and placed marks.
use super::*;

fn attached_pair(entry: &s::ReadEntry, call: u64) -> s::ReadPair {
    let s::ReadSource::Event(statement) = &entry.source else {
        panic!("event statement");
    };
    let started = s::PairStarted {
        call_id: s::CallId {
            run: statement.block_id.run.clone(),
            sequence: statement.block_id.sequence,
            call: call.into(),
        },
        call_block: (call + 1).into(),
        statement: s::StatementRef {
            block_id: statement.block_id.clone(),
            binding: entry.bindings[0].ordinal,
        },
        lane: s::PairLane::External,
        request: s::PairRequest::External {
            name: "sh".into(),
            arguments: "exact 'args'\nline".into(),
        },
        retry_of: None,
    };
    let ended = s::PairEnded {
        call_id: started.call_id.clone(),
        execution: None,
        fate: s::TerminalPairFate::Failed,
        reason: Some(s::PairFailure::NotDispatched {
            cause: s::NonDispatchCause::Stop,
        }),
        outcome: None,
        loaded_from: None,
    };
    let source = |sequence: u64, kind: &str| {
        let mut source = (**statement).clone();
        source.block_id.sequence = sequence.into();
        source.block_id.block = 0.into();
        source.envelope.sequence = sequence.into();
        source.envelope.kind = kind.into();
        source.source_path.clear();
        source
    };
    s::ReadPair {
        call_id: started.call_id.clone(),
        statement: started.statement.clone(),
        lane: started.lane,
        request: started.request.clone(),
        fate: s::PairFate::Failed,
        reason: ended.reason.clone(),
        opening_source: s::PairReadSource {
            source: source(80 + call * 2, "tool.call.started"),
            raw_block: serde_json::value::to_raw_value(&started).unwrap(),
        },
        terminal_source: Some(s::PairReadSource {
            source: source(81 + call * 2, "tool.call.completed"),
            raw_block: serde_json::value::to_raw_value(&ended).unwrap(),
        }),
    }
}

#[test]
fn place_whole_pair_maps_original_call_and_return_before_its_placed_end_mark() {
    let mut fx = PlaceFixture::new();
    let span = s::SpanRef {
        seed_id: fx.summary.seed().into(),
        local_id: 80.into(),
        revision: 1.into(),
    };
    let mut entry = text_entry(8, "record");
    let pair = attached_pair(&entry, 4);
    let call_id = pair.call_id.clone();
    let returned = pair
        .terminal_source
        .as_ref()
        .unwrap()
        .raw_block
        .get()
        .to_owned();
    entry.pairs.push(pair);
    entry.selected_span = Some(s::SelectedSpan {
        span_ref: span.clone(),
        start: 0.into(),
        end: (entry.text.len() as u64).into(),
    });
    let units = fx.material(
        s::ReadSelector::Span {
            seed_id: span.seed_id.clone(),
            local_id: span.local_id,
            revision: span.revision,
        },
        vec![entry],
    );
    assert_eq!(units.len(), 1);
    assert_eq!(units[0].messages().len(), 3);
    let expected = units[0].messages().to_vec();
    assert_eq!(
        expected[2].content,
        vec![ContentBlock::ToolResult(weaver_traits::ToolResultBlock {
            content: returned
        })]
    );
    fx.turn("pair-placement");
    let before = fx.journal();
    fx.generation(units, 7, 0, "", Finish::Completed);
    let record = fx.records(Kind::StatePlacement).pop().unwrap();
    let map = &record["payload"]["unit_map"][0]["source_map"];
    assert_eq!(map.as_array().unwrap().len(), 3);
    assert_eq!(
        map[1]["material"],
        serde_json::to_value(s::MaterialRef::Request {
            call_id: call_id.clone()
        })
        .unwrap()
    );
    assert_eq!(
        map[2]["material"],
        serde_json::to_value(s::MaterialRef::Returned { call_id }).unwrap()
    );
    assert_eq!(map[1]["message_id"], record["payload"]["message_ids"][1]);
    assert_eq!(map[2]["message_id"], record["payload"]["message_ids"][2]);
    assert!(map[1]["start"].is_null());
    assert!(map[1]["end"].is_null());
    let mark = fx.records(Kind::StateMarkPosition).pop().unwrap();
    assert_eq!(
        mark["payload"]["end"]["message"]["message_id"],
        record["payload"]["message_ids"][2]
    );
    assert_eq!(mark["payload"]["end"]["message"]["block_end"], "1");
    assert!(mark["payload"]["end"]["message"]["byte_end"].is_null());
    fx.close_turn();
    let cut = fx.mark(80);
    let prefix = fx.prefix(Some(cut), s::CutMethod::Flush);
    assert_eq!(prefix.n(), before.0);
    assert_eq!(
        prefix.rebuild(),
        expected,
        "cut preserves request and complete return together"
    );
}

#[test]
fn place_repeated_closed_span_keeps_each_valid_occurrence_until_an_actual_cut() {
    let mut fx = PlaceFixture::new();
    let one = fx.whole_span(8, "record", 80);
    let two = fx.whole_span(8, "record", 80);
    fx.turn("repeated-reminder");
    fx.generation(one, 3, 0, "", Finish::Completed);
    let first = fx.mark(80);
    fx.generation(two, 3, 0, "", Finish::Completed);
    let second = fx.mark(80);
    assert_ne!(first.occurrence_id, second.occurrence_id);
    assert_eq!(first.span, second.span);
    let positions = fx.journal().7;
    assert!(
        positions
            .iter()
            .any(|(ordinal, _, _)| *ordinal == first.occurrence_id.ordinal.0)
    );
    assert!(
        positions
            .iter()
            .any(|(ordinal, _, _)| *ordinal == second.occurrence_id.ordinal.0)
    );
    fx.close_turn();
    let prefix = fx.prefix(Some(first.clone()), s::CutMethod::Flush);
    let staged = fx.staged(prefix, ReloadTail::held(vec![]), 3);
    assert_eq!(
        fx.journal()
            .7
            .iter()
            .filter(|(_, span, _)| span == &first.span)
            .count(),
        1
    );
    fx.turn("repeated-reminder-continue");
    fx.begin_staged(staged, vec![]);
    let TokenDirective::MeasureDelta { keep, .. } = fx.next_measure() else {
        unreachable!()
    };
    fx.answer(&fx.measured(keep, 3));
    let PlacementCompletion::Granted(grant) = fx.take() else {
        panic!("retained reminder refused");
    };
    fx.start_generation(grant);
    fx.sent_generation();
    fx.answer(&TokenAnswer::Generated(generated(
        keep,
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
    assert_eq!(
        fx.journal()
            .7
            .iter()
            .filter(|(_, span, _)| span == &first.span)
            .count(),
        1,
        "rebuilding existing messages must not mint another placement mark"
    );
}

#[test]
fn place_nonprefix_utf8_source_range_survives_canonical_render_map_and_rebuild() {
    let mut fx = PlaceFixture::new();
    let span = s::SpanRef {
        seed_id: fx.summary.seed().into(),
        local_id: 80.into(),
        revision: 1.into(),
    };
    let mut entry = text_entry(8, "caf\u{e9}");
    let selected = entry.text.clone();
    let offset = "Read old. ".len() as u64;
    entry.text = format!("Read old. {selected}");
    entry.raw_block =
        serde_json::value::to_raw_value(&serde_json::json!({"type":"text","text":entry.text}))
            .unwrap();
    let binding = &mut entry.bindings[0];
    binding.ordinal = 1.into();
    binding.value.statement_span.start.0 += offset;
    binding.value.statement_span.end.0 += offset;
    binding.value.verb_span.start.0 += offset;
    binding.value.verb_span.end.0 += offset;
    binding.value.nouns[0].span.start.0 += offset;
    binding.value.nouns[0].span.end.0 += offset;
    entry.selected_span = Some(s::SelectedSpan {
        span_ref: span.clone(),
        start: offset.into(),
        end: (entry.text.len() as u64).into(),
    });
    let units = fx.material(
        s::ReadSelector::Span {
            seed_id: span.seed_id,
            local_id: span.local_id,
            revision: span.revision,
        },
        vec![entry],
    );
    assert_eq!(
        units[0].messages()[0].content,
        vec![ContentBlock::Text {
            text: selected.clone()
        }]
    );
    fx.turn("utf8-slice");
    fx.generation(units, 3, 0, "", Finish::Completed);
    let record = fx.records(Kind::StatePlacement).pop().unwrap();
    let map = &record["payload"]["unit_map"][0]["source_map"][0];
    assert_eq!(map["material"]["text"]["span"]["start"], offset.to_string());
    assert_eq!(
        map["material"]["text"]["span"]["end"],
        (offset + selected.len() as u64).to_string()
    );
    assert_eq!(map["start"], "0");
    assert_eq!(map["end"], selected.len().to_string());
    fx.close_turn();
    let cut = fx.mark(80);
    let prefix = fx.prefix(Some(cut), s::CutMethod::Flush);
    assert_eq!(
        prefix.rebuild()[0].content,
        vec![ContentBlock::Text { text: selected }]
    );
}

fn pair_member(fx: &mut PlaceFixture, bytes: &mut Vec<u8>, requests: &mut Vec<s::StateFrame>) {
    fx.summary.transport(TransportSource::State);
    let mut chunk = [0; 65536];
    loop {
        match fx.summary.boot.member.run.state.read(&mut chunk) {
            Ok(0) => panic!("pair member closed"),
            Ok(n) => bytes.extend_from_slice(&chunk[..n]),
            Err(e) if e.kind() == io::ErrorKind::WouldBlock => break,
            Err(e) => panic!("pair member: {e}"),
        }
    }
    while let Some(end) = bytes.iter().position(|b| *b == b'\n') {
        let line: Vec<_> = bytes.drain(..=end).collect();
        let request = s::decode_state_line(&line, s::FrameDirection::Request).unwrap();
        let block_id = match &request {
            s::StateFrame::File { filing, .. } => filing.block_id.clone(),
            s::StateFrame::FilePair { filing, .. } => filing.block_id.clone(),
            _ => panic!("pair fixture only answers actual filings"),
        };
        let began = fx.summary.boot.member.now() + 1;
        let observation = operation(&request, fx.summary.boot.member.next_operation, began);
        fx.summary.boot.member.next_operation += 1;
        fx.summary
            .boot
            .member
            .receive(&s::StateFrame::Filed {
                common: request.common().clone(),
                block_id,
                observation,
            })
            .unwrap();
        requests.push(request);
    }
}

fn actual_completed_pair(fx: &mut PlaceFixture) -> s::CallId {
    use crate::flow::coordinator::{OriginalRequests, RequestScheme};
    let units = fx.text(8, "seed");
    fx.turn("actual-pair");
    fx.hold_files = true;
    let before = fx.journal();
    let grant = fx.grant(units, 3);
    fx.start_generation(grant);
    fx.sent_generation();
    let mut answer = generated(before.1, 3, 2, "Read files.", Finish::Completed, before.2);
    answer
        .content
        .push(ContentBlock::ToolCall(weaver_traits::ToolCall {
            name: "sh".into(),
            arguments: "ls".into(),
        }));
    fx.answer(&TokenAnswer::Generated(answer));
    let GenerationCompletion::Generated(mut generation) = fx.finish_generation() else {
        panic!("actual request generation missing");
    };
    let checked = generation.take_output().unwrap();
    let seed: s::SeedRequest = s::decode_strict(include_str!("fixtures/seed.json")).unwrap();
    let declaration = s::SchemeDeclaration {
        artifact_sha256: s::Digest::new(seed.scheme_id.strip_prefix("ste100:").unwrap()).unwrap(),
        scheme_id: seed.scheme_id,
        verbs: vec!["read".into()],
    };
    let scheme = RequestScheme::from_exact_scheme(&seed.ste100_scheme, &declaration).unwrap();
    let original = OriginalRequests::from_checked(checked, &scheme).unwrap();
    assert_eq!(original.requests().len(), 1);
    let call = original.requests()[0].opening().call_id.clone();
    assert!(
        fx.run().context.hold_place_return(&call).is_err(),
        "generation/CallId alone is not a completed pair"
    );
    fx.decision();
    let run = fx.run();
    run.context
        .begin_not_dispatched(original, s::NonDispatchCause::Stop, &mut run.clock)
        .unwrap();
    assert!(matches!(
        run.context.pairs.take_result(&call),
        Err(pairs::PairError::NotReady)
    ));
    let mut bytes = vec![];
    let mut requests = vec![];
    let mut done = false;
    for _ in 0..256 {
        let run = fx.run();
        run.context
            .advance_pairs(&mut ports(&mut run.clock))
            .unwrap();
        fx.classifier(false);
        pair_member(fx, &mut bytes, &mut requests);
        let run = fx.run();
        if run
            .context
            .take_pair_accounting(&mut run.clock)
            .unwrap()
            .is_some()
        {
            done = true;
            break;
        }
    }
    assert!(
        done,
        "actual original statement/opening/terminal receipts must settle"
    );
    assert_eq!(
        requests
            .iter()
            .filter(|request| matches!(request, s::StateFrame::FilePair { .. }))
            .count(),
        2
    );
    fx.hold_files = false;
    call
}

#[test]
fn place_return_only_requires_actual_completed_pair_and_resident_request_statement_coverage() {
    for remove_request in [false, true] {
        let mut fx = PlaceFixture::new();
        let call = actual_completed_pair(&mut fx);
        let returned = fx
            .run()
            .context
            .hold_place_return(&call)
            .expect("actual terminal receipt licenses the unit");
        assert_eq!(returned.messages().len(), 1);
        assert!(matches!(
            returned.messages()[0].content.as_slice(),
            [ContentBlock::ToolResult(_)]
        ));
        if remove_request {
            fx.close_turn();
            let boot = fx
                .records(Kind::StateMarkPosition)
                .into_iter()
                .next()
                .unwrap();
            let cut = s::CutReference {
                span: serde_json::from_value(boot["payload"]["span"].clone()).unwrap(),
                occurrence_id: serde_json::from_value(boot["payload"]["id"].clone()).unwrap(),
            };
            let prefix = fx.prefix(Some(cut), s::CutMethod::Flush);
            let staged = fx.staged(prefix, ReloadTail::held(vec![]), 0);
            fx.turn("return-after-request-cut");
            fx.decision();
            let run = &mut fx.summary.boot.member.run;
            let error = run
                .context
                .begin_staged_place(
                    staged,
                    vec![returned],
                    s::PlacementContext {
                        label: "orphan return".into(),
                        instruction: None,
                    },
                    &mut fx.decode,
                    &mut run.clock,
                )
                .unwrap_err();
            assert_eq!(error.refusal, PlaceError::InvalidUnits);
            assert_eq!(error.selection.len(), 1);
            assert!(receive(fx.peer.as_fd()).is_none());
            assert_eq!(fx.finish_notice().code, s::PlacementCode::InvalidPair);
        } else {
            let run = fx.run();
            let prefix = run
                .context
                .prepare_prefix(None, s::CutMethod::Flush, &mut run.clock)
                .unwrap();
            assert!(prefix.coverage().contains(&s::MaterialRef::Request {
                call_id: call.clone()
            }));
            let generated = fx.generation(vec![returned], 2, 0, "", Finish::Completed);
            let record = fx.records(Kind::StatePlacement).pop().unwrap();
            assert_eq!(
                record["payload"]["unit_map"][0]["source_map"][0]["material"],
                serde_json::to_value(s::MaterialRef::Returned { call_id: call }).unwrap()
            );
            assert!(generated.generation().resident > 0);
        }
    }
}

#[test]
fn return_carry_acknowledges_only_the_actual_completed_return_and_refuses_stale_units() {
    let mut fx = PlaceFixture::new();
    let call = actual_completed_pair(&mut fx);
    let mut unknown = call.clone();
    unknown.call.0 += 100;
    assert!(matches!(
        fx.run().context.pairs.take_result(&unknown),
        Err(pairs::PairError::UnknownCall)
    ));
    let returned = fx.run().context.hold_place_return(&call).unwrap();
    let stale = fx.run().context.hold_place_return(&call).unwrap();
    let unrelated = fx.text(9, "unrelated");
    let other = fx.generation(unrelated, 2, 0, "", Finish::Completed);
    assert_eq!(
        fx.run().context.acknowledge_delivery(&call, &other),
        Err(pairs::PairError::WrongReceipt)
    );
    let proof = fx.generation(vec![returned], 2, 0, "", Finish::Completed);
    let record = fx.records(Kind::MessageToolResult);
    assert_eq!(record.len(), 1);
    // The record now exists: an older unrelated append must still fail the
    // matching returned-coverage guard, independently of that earlier gate.
    assert_eq!(
        fx.run().context.acknowledge_delivery(&call, &other),
        Err(pairs::PairError::WrongReceipt)
    );
    let placement = fx.records(Kind::StatePlacement).pop().unwrap();
    assert!(
        record[0]["sequence"]
            .as_str()
            .unwrap()
            .parse::<u64>()
            .unwrap()
            < placement["sequence"]
                .as_str()
                .unwrap()
                .parse::<u64>()
                .unwrap()
    );
    assert_eq!(
        record[0]["payload"],
        serde_json::to_value(&proof.grant.delta[0]).unwrap()
    );
    let saved = fx
        .run()
        .context
        .pairs
        .take_result(&call)
        .unwrap()
        .ended()
        .clone();
    fx.run()
        .context
        .acknowledge_delivery(&call, &proof)
        .unwrap();
    assert!(matches!(
        fx.run().context.pairs.take_result(&call),
        Err(pairs::PairError::AlreadyDelivered)
    ));
    assert_eq!(
        fx.run()
            .context
            .pairs
            .completed(&call)
            .unwrap()
            .unwrap()
            .ended(),
        &saved
    );
    assert_eq!(
        fx.run().context.acknowledge_delivery(&call, &proof),
        Err(pairs::PairError::AlreadyDelivered)
    );
    assert!(fx.run().context.hold_place_return(&call).is_err());
    let request = fx.request(vec![stale]);
    let run = &mut fx.summary.boot.member.run;
    let error = run
        .context
        .begin_place(request, &mut fx.decode, &mut run.clock)
        .unwrap_err();
    assert_eq!(error.refusal, PlaceError::InvalidUnits);
    assert_eq!(error.input.selection.len(), 1);
    assert!(receive(fx.peer.as_fd()).is_none());
}

#[test]
fn return_carry_cancelled_placement_and_stopped_turn_retry_without_another_result_record() {
    let mut fx = PlaceFixture::new();
    let call = actual_completed_pair(&mut fx);
    let returned = fx.run().context.hold_place_return(&call).unwrap();
    let grant = fx.grant(vec![returned], 2);
    let records = fx.records(Kind::MessageToolResult);
    assert_eq!(records.len(), 1);
    let account = fx.run().context.cancel_placement_grant(grant).unwrap();
    assert!(account.confirmed_generation().is_none());
    assert!(fx.run().context.pairs.take_result(&call).is_ok());
    fx.observe_place_command(weaver_types::LifecycleDirective::Stop {
        cause: weaver_types::Cause { uid: 73 },
    });
    let run = fx.run();
    run.author(AuthoredEvent {
        kind: Kind::TurnClosed,
        subsystem: weaver_trace::Subsystem::Harness,
        turn: run.context.open_turn.clone(),
        source: AuthoredSource::Runtime,
        causal_parent: None,
        payload: Some(Payload::TurnClosed(weaver_trace::TurnClose::Stopped {
            reason: weaver_trace::StopReason::Directive,
            cause: Some(weaver_trace::Cause { uid: 73 }),
        })),
    })
    .unwrap();
    fx.turn("next-turn-return");
    let returned = fx.run().context.hold_place_return(&call).unwrap();
    let proof = fx.generation(vec![returned], 2, 0, "", Finish::Completed);
    fx.run()
        .context
        .acknowledge_delivery(&call, &proof)
        .unwrap();
    assert_eq!(fx.records(Kind::MessageToolResult), records);
    assert_eq!(fx.records(Kind::ToolCallStarted).len(), 1);
    assert_eq!(fx.records(Kind::ToolCallCompleted).len(), 1);
}

#[test]
fn return_carry_without_append_receipt_cannot_acknowledge_even_matching_result_content() {
    let mut fx = PlaceFixture::new();
    let call = actual_completed_pair(&mut fx);
    let returned = fx.run().context.hold_place_return(&call).unwrap();
    let mut proof = fx.generation(vec![returned], 2, 0, "", Finish::Completed);
    // A valid decoder answer is constructed before the journal completes it.
    // Strip only that journal receipt to exercise this distinct evidence gate.
    let receipt = proof.append_receipt.take().unwrap();
    assert_eq!(
        fx.run().context.acknowledge_delivery(&call, &proof),
        Err(pairs::PairError::WrongReceipt)
    );
    assert!(fx.run().context.pairs.take_result(&call).is_ok());
    proof.append_receipt = Some(receipt);
    fx.run()
        .context
        .acknowledge_delivery(&call, &proof)
        .unwrap();
}

#[test]
fn return_carry_foreign_context_cannot_acknowledge_matching_call_numbers() {
    let mut first = PlaceFixture::new();
    let call = actual_completed_pair(&mut first);
    let returned = first.run().context.hold_place_return(&call).unwrap();
    let proof = first.generation(vec![returned], 2, 0, "", Finish::Completed);
    let mut second = PlaceFixture::new();
    let second_call = actual_completed_pair(&mut second);
    assert_eq!(
        call, second_call,
        "identical public identities do not grant custody"
    );
    assert_eq!(
        second.run().context.acknowledge_delivery(&call, &proof),
        Err(pairs::PairError::WrongReceipt)
    );
    assert!(second.run().context.pairs.take_result(&call).is_ok());
}

#[test]
fn return_carry_trace_pressure_cancellation_keeps_the_grant_and_original_result_author() {
    let mut fx = PlaceFixture::new();
    let call = actual_completed_pair(&mut fx);
    let returned = fx.run().context.hold_place_return(&call).unwrap();
    let request = fx.request(vec![returned]);
    fx.begin(request);
    let TokenDirective::MeasureDelta { keep, .. } = fx.next_measure() else {
        unreachable!()
    };
    fx.answer(&fx.measured(keep, 2));
    fx.summary.boot.member.fill_trace();
    let mut ticket = None;
    for _ in 0..128 {
        fx.advance_placement_pressure().unwrap();
        if let Some(author) = fx.run().context.pending_author() {
            assert_eq!(author.event.kind, Kind::MessageToolResult);
            ticket = Some(author.ticket);
            break;
        }
    }
    let ticket = ticket.expect("the exact result author is blocked by trace pressure");
    for _ in 0..4 {
        assert_ne!(
            fx.advance_placement_pressure().unwrap(),
            PlaceProgress::Complete
        );
        assert_eq!(fx.run().context.pending_author().unwrap().ticket, ticket);
        assert!(fx.run().context.pairs.take_result(&call).is_ok());
        assert!(receive(fx.peer.as_fd()).is_none());
    }
    fx.run().context.revoke_place();
    fx.summary.drain();
    assert!(matches!(fx.take(), PlacementCompletion::Cancelled(_)));
    let original = fx.records(Kind::MessageToolResult);
    assert_eq!(original.len(), 1);
    let returned = fx.run().context.hold_place_return(&call).unwrap();
    let proof = fx.generation(vec![returned], 2, 0, "", Finish::Completed);
    fx.run()
        .context
        .acknowledge_delivery(&call, &proof)
        .unwrap();
    assert_eq!(fx.records(Kind::MessageToolResult), original);
}

#[test]
fn return_carry_refused_measurement_keeps_result_for_the_next_turn() {
    let mut fx = PlaceFixture::new();
    let call = actual_completed_pair(&mut fx);
    let returned = fx.run().context.hold_place_return(&call).unwrap();
    let request = fx.request(vec![returned]);
    fx.begin(request);
    let TokenDirective::MeasureDelta { keep, .. } = fx.next_measure() else {
        unreachable!()
    };
    fx.answer(&fx.measured(keep, 20000));
    assert!(matches!(fx.take(), PlacementCompletion::Refused(_)));
    assert!(fx.run().context.pairs.take_result(&call).is_ok());
    assert!(fx.records(Kind::MessageToolResult).is_empty());
    let run = fx.run();
    run.context.place_response_sent(&mut run.clock).unwrap();
    fx.turn("after-refusal");
    let returned = fx.run().context.hold_place_return(&call).unwrap();
    let proof = fx.generation(vec![returned], 2, 0, "", Finish::Completed);
    fx.run()
        .context
        .acknowledge_delivery(&call, &proof)
        .unwrap();
    assert_eq!(fx.records(Kind::MessageToolResult).len(), 1);
    assert_eq!(fx.records(Kind::ToolCallCompleted).len(), 1);
}
