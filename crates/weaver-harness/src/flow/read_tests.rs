use super::*;
use weaver_types::state_flow::{
    Binding, EventSource, NounBinding, ReadBinding, ReadEntry, ReadSource, SourceBlockId,
    SourceEnvelope, SourcePathPart, Span,
};

fn selector() -> ReadSelector {
    ReadSelector::History {
        noun: "file".into(),
        verb: None,
        condition: None,
    }
}

fn empty() -> Vec<u8> {
    br#"{"entries":{"entries":[]}}"#.to_vec()
}

fn encode(bytes: &[u8]) -> String {
    const DIGITS: &[u8] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut result = String::new();
    for chunk in bytes.chunks(3) {
        let a = chunk[0];
        let b = chunk.get(1).copied().unwrap_or(0);
        let c = chunk.get(2).copied().unwrap_or(0);
        result.push(DIGITS[usize::from(a >> 2)] as char);
        result.push(DIGITS[usize::from((a & 3) << 4 | b >> 4)] as char);
        result.push(if chunk.len() > 1 {
            DIGITS[usize::from((b & 15) << 2 | c >> 6)] as char
        } else {
            '='
        });
        result.push(if chunk.len() > 2 {
            DIGITS[usize::from(c & 63)] as char
        } else {
            '='
        });
    }
    result
}

fn page(bytes: &[u8], offset: usize, end: usize) -> StateControlAnswer {
    StateControlAnswer::ReadPage(ReadPage {
        snapshot_id: 7.into(),
        offset: (offset as u64).into(),
        total_bytes: (bytes.len() as u64).into(),
        octets: encode(&bytes[offset..end]),
        done: end == bytes.len(),
    })
}

fn refuse(code: ReadRefusalCode) -> StateControlAnswer {
    StateControlAnswer::ControlRefused {
        code: ControlRefusalCode::Read(code),
        detail: "exact store detail".into(),
    }
}

fn round(
    read: &mut PageRead,
    id: u64,
    answer: StateControlAnswer,
) -> Result<ReadAdvance, PageReadError> {
    let control = read.next_control().expect("next control");
    read.bind_request(id.into(), &control).unwrap();
    assert!(read.next_control().is_none());
    read.accept(id.into(), &control, &answer)
}

fn release(read: &mut PageRead, id: u64) {
    assert_eq!(
        read.next_control(),
        Some(StateControl::Release {
            snapshot_id: 7.into()
        })
    );
    assert_eq!(
        round(
            read,
            id,
            StateControlAnswer::Released {
                snapshot_id: 7.into()
            }
        )
        .unwrap(),
        ReadAdvance::Complete
    );
}

fn unavailable(read: &mut PageRead) {
    assert!(read.next_control().is_none());
    assert!(read.take().is_none());
}

#[test]
fn final_page_is_not_a_result_until_the_exact_snapshot_releases_once() {
    let raw = empty();
    let mut read = PageRead::new(selector(), (raw.len() as u64).into()).unwrap();
    assert_eq!(
        round(&mut read, 11, page(&raw, 0, raw.len())).unwrap(),
        ReadAdvance::Pending
    );
    assert!(read.take().is_none());
    release(&mut read, 17);
    let completed = read.take().unwrap().unwrap();
    assert_eq!(completed.selector(), &selector());
    assert_eq!(completed.snapshot_id(), 7.into());
    assert_eq!(completed.raw_bytes(), raw);
    assert!(matches!(completed.result(), QueryResult::Entries { entries } if entries.is_empty()));
    unavailable(&mut read);
}

#[test]
fn every_initial_read_refusal_preserves_code_detail_and_acquires_no_snapshot() {
    for code in [
        ReadRefusalCode::UnsupportedOperation,
        ReadRefusalCode::InvalidSelector,
        ReadRefusalCode::WrongSeed,
        ReadRefusalCode::NotFound,
        ReadRefusalCode::UnknownSpan,
        ReadRefusalCode::SpanOpen,
        ReadRefusalCode::SpanPending,
        ReadRefusalCode::PartialPair,
        ReadRefusalCode::CounterExhausted,
        ReadRefusalCode::ReadBusy,
        ReadRefusalCode::UnknownSnapshot,
        ReadRefusalCode::WrongOffset,
        ReadRefusalCode::ResultTooLarge,
        ReadRefusalCode::ResultUnavailable,
        ReadRefusalCode::AnswerTooLarge,
    ] {
        let mut read = PageRead::new(selector(), 1_000.into()).unwrap();
        assert_eq!(
            round(&mut read, 1, refuse(code)).unwrap(),
            ReadAdvance::Complete
        );
        assert!(read.snapshot.is_none());
        assert!(read.next_control().is_none());
        assert_eq!(
            read.take().unwrap().unwrap_err(),
            ReadFailure::Refused {
                code,
                detail: "exact store detail".into(),
            }
        );
        unavailable(&mut read);
    }
}

#[test]
fn later_refusal_owns_the_snapshot_until_one_release_and_keeps_original_detail() {
    let raw = empty();
    let mut read = PageRead::new(selector(), 1_000.into()).unwrap();
    round(&mut read, 1, page(&raw, 0, 3)).unwrap();
    assert_eq!(
        read.next_control(),
        Some(StateControl::Next {
            snapshot_id: 7.into(),
            offset: 3.into()
        })
    );
    assert_eq!(
        round(&mut read, 2, refuse(ReadRefusalCode::ResultUnavailable)).unwrap(),
        ReadAdvance::Pending
    );
    assert!(read.take().is_none());
    release(&mut read, 3);
    assert_eq!(
        read.take().unwrap().unwrap_err(),
        ReadFailure::Refused {
            code: ReadRefusalCode::ResultUnavailable,
            detail: "exact store detail".into(),
        }
    );
    unavailable(&mut read);
}

#[test]
fn cleanup_refusal_is_protocol_failure_and_cannot_escape_as_the_original_refusal() {
    let raw = empty();
    let mut read = PageRead::new(selector(), 1_000.into()).unwrap();
    round(&mut read, 1, page(&raw, 0, 3)).unwrap();
    round(&mut read, 2, refuse(ReadRefusalCode::ResultUnavailable)).unwrap();
    assert!(round(&mut read, 3, refuse(ReadRefusalCode::UnknownSnapshot)).is_err());
    assert_eq!(read.snapshot, Some(7.into()));
    assert_eq!(read.bytes, raw[..3]);
    unavailable(&mut read);
}

#[test]
fn page_snapshot_offset_total_and_done_must_stay_exact() {
    let raw = empty();
    for defect in 0..5 {
        let mut read = PageRead::new(selector(), 1_000.into()).unwrap();
        round(&mut read, 1, page(&raw, 0, 3)).unwrap();
        let StateControlAnswer::ReadPage(mut second) = page(&raw, 3, raw.len()) else {
            unreachable!()
        };
        match defect {
            0 => second.snapshot_id = 8.into(),
            1 => second.offset = 2.into(),
            2 => second.total_bytes = (raw.len() as u64 + 1).into(),
            3 => second.done = false,
            _ => second.octets = "!!!!".into(),
        }
        assert!(round(&mut read, 2, StateControlAnswer::ReadPage(second)).is_err());
        assert_eq!(read.snapshot, Some(7.into()));
        assert_eq!(read.bytes, raw[..3]);
        unavailable(&mut read);
    }
}

#[test]
fn an_oversized_declared_total_is_refused_before_any_reassembly_allocation() {
    // The small excess detects a removed bound even when a huge reservation
    // would independently fail. Both must refuse before reserving assembly.
    for total in [21, u64::MAX] {
        let mut read = PageRead::new(selector(), 20.into()).unwrap();
        let answer = StateControlAnswer::ReadPage(ReadPage {
            snapshot_id: 7.into(),
            offset: 0.into(),
            total_bytes: total.into(),
            octets: encode(b"x"),
            done: false,
        });
        assert!(round(&mut read, 1, answer).is_err());
        assert_eq!(read.bytes.capacity(), 0);
        assert!(read.snapshot.is_none());
        unavailable(&mut read);
    }
}

#[test]
fn initial_page_requires_nonzero_snapshot_and_zero_offset() {
    let raw = empty();
    for defect in 0..3 {
        let mut read = PageRead::new(selector(), 1_000.into()).unwrap();
        let StateControlAnswer::ReadPage(mut first) = page(&raw, 0, raw.len()) else {
            unreachable!()
        };
        match defect {
            0 => first.snapshot_id = 0.into(),
            1 => first.offset = 1.into(),
            _ => first.total_bytes = 0.into(),
        }
        assert!(round(&mut read, 1, StateControlAnswer::ReadPage(first)).is_err());
        assert!(read.snapshot.is_none());
        unavailable(&mut read);
    }
}

#[test]
fn wrong_or_missing_request_correlation_never_consumes_an_answer() {
    let raw = empty();
    for defect in 0..3 {
        let mut read = PageRead::new(selector(), 1_000.into()).unwrap();
        let expected = read.next_control().unwrap();
        if defect != 0 {
            read.bind_request(8.into(), &expected).unwrap();
        }
        let id = if defect == 1 { 9 } else { 8 };
        let control = if defect == 2 {
            StateControl::Held {}
        } else {
            expected
        };
        assert!(
            read.accept(id.into(), &control, &page(&raw, 0, raw.len()))
                .is_err()
        );
        assert!(read.snapshot.is_none());
        unavailable(&mut read);
    }
}

#[test]
fn a_second_start_or_reused_request_id_cannot_create_a_second_leg() {
    let raw = empty();
    let mut read = PageRead::new(selector(), 1_000.into()).unwrap();
    let expected = read.next_control().unwrap();
    read.bind_request(8.into(), &expected).unwrap();
    assert!(read.bind_request(9.into(), &expected).is_err());
    unavailable(&mut read);

    let mut read = PageRead::new(selector(), 1_000.into()).unwrap();
    round(&mut read, 8, page(&raw, 0, raw.len())).unwrap();
    let release = read.next_control().unwrap();
    assert!(read.bind_request(8.into(), &release).is_err());
    unavailable(&mut read);
}

#[test]
fn malformed_json_duplicate_members_and_wrong_result_tag_never_release_or_grant() {
    for raw in [
        br#"{"entries":{"entries":[],"entries":[]}}"#.as_slice(),
        br#"{"entries":{"entries":[]}} false"#.as_slice(),
        br#"{"open_pairs":{"pairs":[]}}"#.as_slice(),
        b"\xff".as_slice(),
    ] {
        let mut read = PageRead::new(selector(), 1_000.into()).unwrap();
        assert!(round(&mut read, 1, page(raw, 0, raw.len())).is_err());
        assert_eq!(read.snapshot, Some(7.into()));
        assert_eq!(read.bytes, raw);
        unavailable(&mut read);
    }
}

#[test]
fn premature_or_wrong_snapshot_release_cannot_publish_the_result() {
    for ready in [false, true] {
        let raw = empty();
        let mut read = PageRead::new(selector(), 1_000.into()).unwrap();
        if ready {
            round(&mut read, 1, page(&raw, 0, raw.len())).unwrap();
        }
        assert!(
            round(
                &mut read,
                2,
                StateControlAnswer::Released {
                    snapshot_id: if ready { 8.into() } else { 7.into() }
                }
            )
            .is_err()
        );
        unavailable(&mut read);
    }
}

#[test]
fn freezing_at_each_protocol_phase_suppresses_content_and_future_controls() {
    for at in 0..4 {
        let raw = empty();
        let mut read = PageRead::new(selector(), 1_000.into()).unwrap();
        if at >= 1 {
            round(&mut read, 1, page(&raw, 0, 3)).unwrap();
        }
        if at >= 2 {
            round(&mut read, 2, page(&raw, 3, raw.len())).unwrap();
        }
        if at >= 3 {
            release(&mut read, 3);
        }
        read.freeze();
        unavailable(&mut read);
    }
}

fn unicode_result() -> (ReadSelector, Vec<u8>) {
    let text = "Read caf\u{e9}.";
    let span = |start: u64, end: u64| Span {
        start: start.into(),
        end: end.into(),
    };
    let source = SourceBlockId {
        run: "run".into(),
        sequence: 9.into(),
        block: 0.into(),
    };
    let entry = ReadEntry {
        source: ReadSource::Event(Box::new(EventSource {
            block_id: source,
            envelope: SourceEnvelope {
                session: "session".into(),
                run: "run".into(),
                turn: Some("turn".into()),
                sequence: 9.into(),
                kind: "message.assistant".into(),
                channel: "output".into(),
                origin: "model".into(),
                subsystem: "harness".into(),
                causal_parent: None,
                wall_ms: 0,
                monotonic_ns: 9.into(),
            },
            source_path: vec![
                SourcePathPart::Key("content".into()),
                SourcePathPart::Index { index: 0.into() },
            ],
            partition_rule_id: "statements".into(),
        })),
        raw_block: serde_json::value::RawValue::from_string(
            r#"{ "type":"text", "text":"Read caf\u00e9." }"#.into(),
        )
        .unwrap(),
        text: text.into(),
        bindings: vec![ReadBinding {
            ordinal: 3.into(),
            value: Binding {
                statement_span: span(0, text.len() as u64),
                verb: "read".into(),
                verb_span: span(0, 4),
                condition: None,
                condition_span: None,
                nouns: vec![NounBinding {
                    value: "caf\u{e9}".into(),
                    span: span(5, 10),
                }],
            },
        }],
        selected_span: None,
        step_query: None,
        pairs: vec![],
    };
    (
        ReadSelector::History {
            noun: "caf\u{e9}".into(),
            verb: None,
            condition: None,
        },
        serde_json::to_vec(&QueryResult::Entries {
            entries: vec![entry],
        })
        .unwrap(),
    )
}

#[test]
fn pages_may_split_utf8_and_preserve_raw_value_spelling_and_binding_ordinal() {
    let (selector, raw) = unicode_result();
    let split = raw
        .windows(2)
        .position(|part| part == [0xc3, 0xa9])
        .unwrap()
        + 1;
    assert!(std::str::from_utf8(&raw[..split]).is_err());
    let mut read = PageRead::new(selector, (raw.len() as u64).into()).unwrap();
    round(&mut read, 4, page(&raw, 0, split)).unwrap();
    assert!(read.take().is_none());
    round(&mut read, 5, page(&raw, split, raw.len())).unwrap();
    assert!(read.take().is_none());
    release(&mut read, 6);
    let completed = read.take().unwrap().unwrap();
    assert_eq!(completed.raw_bytes(), raw);
    let QueryResult::Entries { entries } = completed.result() else {
        panic!("entries");
    };
    assert_eq!(entries[0].text, "Read caf\u{e9}.");
    assert_eq!(entries[0].bindings[0].ordinal, 3.into());
    assert!(entries[0].raw_block.get().contains(r#"caf\u00e9"#));
}

#[test]
fn paging_preserves_whole_pair_payload_bytes_and_completed_tool_outcome() {
    use weaver_types::state_flow as s;

    let (selector, raw) = unicode_result();
    let QueryResult::Entries { mut entries } =
        decode_strict(std::str::from_utf8(&raw).unwrap()).unwrap()
    else {
        unreachable!()
    };
    let ReadSource::Event(statement_source) = &entries[0].source else {
        unreachable!()
    };
    let started = s::PairStarted {
        call_id: s::CallId {
            run: "run".into(),
            sequence: 9.into(),
            call: 0.into(),
        },
        call_block: 1.into(),
        statement: s::StatementRef {
            block_id: statement_source.block_id.clone(),
            binding: 3.into(),
        },
        lane: s::PairLane::External,
        request: s::PairRequest::External {
            name: "sh".into(),
            arguments: "cat file".into(),
        },
        retry_of: None,
    };
    let ended = s::PairEnded {
        call_id: started.call_id.clone(),
        execution: Some(weaver_types::ExchangeId {
            opener: weaver_types::Opener::Harness,
            ordinal: 31,
        }),
        fate: s::TerminalPairFate::Completed,
        reason: None,
        outcome: Some(weaver_types::ToolOutcome::Result {
            content: "first line\nlast line \u{e9}".into(),
        }),
        loaded_from: None,
    };
    let source = |sequence: u64, kind: &str| {
        let mut source = statement_source.as_ref().clone();
        source.block_id.sequence = sequence.into();
        source.envelope.sequence = sequence.into();
        source.envelope.kind = kind.into();
        source.source_path.clear();
        source
    };
    let opening_raw = serde_json::to_string_pretty(&started).unwrap();
    let terminal_raw = serde_json::to_string_pretty(&ended).unwrap();
    let opening_source = source(10, "tool.call.started");
    let terminal_source = source(11, "tool.call.completed");
    entries[0].pairs.push(s::ReadPair {
        call_id: started.call_id.clone(),
        statement: started.statement.clone(),
        lane: started.lane,
        request: started.request.clone(),
        fate: s::PairFate::Completed,
        reason: None,
        opening_source: s::PairReadSource {
            source: opening_source,
            raw_block: serde_json::value::RawValue::from_string(opening_raw.clone()).unwrap(),
        },
        terminal_source: Some(s::PairReadSource {
            source: terminal_source,
            raw_block: serde_json::value::RawValue::from_string(terminal_raw.clone()).unwrap(),
        }),
    });
    let raw = serde_json::to_vec(&QueryResult::Entries { entries }).unwrap();
    let split = raw.len() / 2;
    let mut read = PageRead::new(selector, (raw.len() as u64).into()).unwrap();
    round(&mut read, 1, page(&raw, 0, split)).unwrap();
    round(&mut read, 2, page(&raw, split, raw.len())).unwrap();
    release(&mut read, 3);
    let completed = read.take().unwrap().unwrap();
    assert_eq!(completed.raw_bytes(), raw);
    let QueryResult::Entries { entries } = completed.result() else {
        unreachable!()
    };
    let [pair] = entries[0].pairs.as_slice() else {
        panic!("one complete attachment")
    };
    assert_eq!(pair.opening_source.raw_block.get(), opening_raw);
    let terminal = pair.terminal_source.as_ref().unwrap();
    assert_eq!(terminal.raw_block.get(), terminal_raw);
    let retained: s::PairEnded = decode_strict(terminal.raw_block.get()).unwrap();
    assert_eq!(retained, ended);
}
