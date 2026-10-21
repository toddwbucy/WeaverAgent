//! STATE-MODE, STATE-CHANNEL, STATE-PAGES, STATE-OBSERVE and STATE-PERSIST-STATES
//! against the normal member process. Fixtures carry synthetic byte qualification
//! only. No classifier, production seed or capacity claim is made by these tests.
#![cfg(all(target_os = "linux", feature = "sqlite"))]

#[path = "serving_member/fixtures.rs"]
mod fixtures;
#[path = "serving_member/process.rs"]
mod process;

use std::io::Write;

use process::{ImageDescriptor, Member};
use weaver_state::ServingStore;
use weaver_state::engine::serving_sqlite::{ServingSqlite, seed_image};
use weaver_types::state_flow::*;

fn entries(result: QueryResult) -> Vec<ReadEntry> {
    let QueryResult::Entries { entries } = result else {
        panic!("{result:?}");
    };
    entries
}

fn page(answer: StateControlAnswer) -> ReadPage {
    let StateControlAnswer::ReadPage(page) = answer else {
        panic!("{answer:?}");
    };
    page
}

fn read_refused(answer: StateControlAnswer, expected: ReadRefusalCode) {
    assert!(
        matches!(answer, StateControlAnswer::ControlRefused {
        code: ControlRefusalCode::Read(code), .. } if code == expected),
        "{answer:?}"
    );
}

fn capture_refused(answer: StateControlAnswer) {
    assert!(
        matches!(
            answer,
            StateControlAnswer::ControlRefused {
                code: ControlRefusalCode::Capture(CaptureRefusalCode::InvalidCaptureState),
                ..
            }
        ),
        "{answer:?}"
    );
}

fn assert_filed(frame: StateFrame, expected: &SourceBlockId) {
    assert!(
        matches!(frame, StateFrame::Filed { ref block_id, .. } if block_id == expected),
        "{frame:?}"
    );
}

#[test]
fn explicit_modes_refuse_missing_writable_or_nonregular_images_and_preload() {
    for (mode, image, wrong_door, preload) in [
        ("serving", ImageDescriptor::Absent, false, false),
        ("serving", ImageDescriptor::Writable, false, false),
        ("serving", ImageDescriptor::Socket, false, false),
        ("serving", ImageDescriptor::ReadOnly, true, false),
        ("serving", ImageDescriptor::ReadOnly, false, true),
        ("seed", ImageDescriptor::ReadOnly, false, false),
        ("seed", ImageDescriptor::Absent, false, true),
        ("diagnostic", ImageDescriptor::Absent, false, false),
        ("unknown", ImageDescriptor::Absent, false, false),
    ] {
        let mut member = Member::launch(mode, image, wrong_door, preload);
        member.assert_closed();
        assert!(
            member.room_names().is_empty(),
            "refused mode must not write state"
        );
    }
}

#[test]
fn mode_never_changes_to_follow_the_first_frame() {
    let mut serving = Member::launch("serving", ImageDescriptor::ReadOnly, false, false);
    let seed = SeedFrame::Seed {
        request: Box::new(serving.seed.clone()),
    };
    serving
        .channel
        .get_mut()
        .write_all(&encode_seed_line(&seed, FrameDirection::Request).unwrap())
        .unwrap();
    serving.assert_closed();
    assert!(serving.room_names().is_empty());

    let mut seeding = Member::launch("seed", ImageDescriptor::Absent, false, false);
    let open = seeding.open_request();
    seeding.send(&open);
    seeding.assert_closed();
    assert!(seeding.room_names().is_empty());
}

#[test]
fn file_before_open_and_second_open_close_without_acceptance() {
    let mut member = Member::launch("serving", ImageDescriptor::ReadOnly, false, false);
    let request = StateFrame::File {
        common: member.common(),
        filing: fixtures::statement(&member.seed, 1, "Read item."),
    };
    member.send(&request);
    member.assert_closed();

    let mut member = Member::serving();
    let mut request = member.open_request();
    let StateFrame::Open { common, .. } = &mut request else {
        unreachable!()
    };
    common.request_id = 1.into();
    member.send(&request);
    member.assert_closed();
}

#[test]
fn a_control_cannot_change_the_opened_session_run_or_request_order() {
    for changed in 0..3 {
        let mut member = Member::serving();
        let mut common = member.common();
        match changed {
            0 => common.session_id = "another-session".into(),
            1 => common.run_id = "another-run".into(),
            _ => common.request_id = 4.into(),
        }
        member.send(&StateFrame::Control {
            common,
            frame: StateControl::Held {},
        });
        member.assert_closed();
        assert!(member.room_names().is_empty());
    }
}

#[test]
fn malformed_duplicate_and_oversized_frames_close_instead_of_minting_receipts() {
    for variant in 0..4 {
        let mut member = Member::launch("serving", ImageDescriptor::ReadOnly, false, false);
        let request = member.open_request();
        let valid =
            String::from_utf8(encode_state_line(&request, FrameDirection::Request).unwrap())
                .unwrap();
        let bytes = match variant {
            0 => valid
                .replacen("\"request_id\":\"1\"", "\"request_id\":\"0\"", 1)
                .into_bytes(),
            1 => valid
                .replacen("\"seed_id\":", "\"seed_id\":\"duplicate\",\"seed_id\":", 1)
                .into_bytes(),
            2 => valid
                .replacen("\"op\":\"open\"", "\"op\":\"not-an-operation\"", 1)
                .into_bytes(),
            _ => {
                let mut bytes = vec![b' '; STATE_REQUEST_LINE_BOUND];
                bytes.push(b'\n');
                bytes
            }
        };
        // An over-bound peer may close while the test is still sending its tail.
        let sent = member.channel.get_mut().write_all(&bytes);
        if let Err(error) = sent {
            assert!(matches!(
                error.kind(),
                std::io::ErrorKind::BrokenPipe | std::io::ErrorKind::ConnectionReset
            ));
        }
        member.assert_closed();
    }
}

#[test]
fn seed_is_held_as_raw_sqlite_then_deleted_on_eof_or_a_second_request() {
    for second_request in [false, true] {
        let mut member = Member::launch("seed", ImageDescriptor::Absent, false, false);
        let request = SeedFrame::Seed {
            request: Box::new(member.seed.clone()),
        };
        let bytes = encode_seed_line(&request, FrameDirection::Request).unwrap();
        member.channel.get_mut().write_all(&bytes).unwrap();
        let answer = decode_seed_line(&member.line(), FrameDirection::Response).unwrap();
        answer.validate_response_to(&member.seed).unwrap();
        let SeedFrame::Created { created } = answer else {
            panic!("{answer:?}");
        };
        assert_eq!(
            created.room_name,
            format!("seed-{}.sqlite", member.seed.seed_id)
        );
        let image = std::fs::read(member.room().join(&created.room_name)).unwrap();
        assert_eq!(image.len() as u64, created.size_bytes.0);
        assert_eq!(&image[..16], b"SQLite format 3\0");
        let store = ServingSqlite::adopt(
            &image,
            &fixtures::common(1),
            &member.seed.schema_id,
            &member.seed.scheme_id,
            &fixtures::start(&image, &member.seed),
            fixtures::limits(1_000_000),
        )
        .unwrap();
        let prompt = entries(store.query(&ReadSelector::SystemPrompt {}).unwrap());
        assert_eq!(prompt.len(), 1);
        assert_eq!(prompt[0].text, member.seed.prompt_text);
        assert!(prompt[0].selected_span.is_some());
        assert_eq!(store.held().unwrap().0, 1);
        assert_eq!(store.position().unwrap(), None);
        if second_request {
            member.channel.get_mut().write_all(&bytes).unwrap();
            member.assert_closed();
        } else {
            member.shutdown();
        }
        assert!(
            member.room_names().is_empty(),
            "seed image outlived its held channel"
        );
    }
}

#[test]
fn seed_certificate_failure_never_creates_a_room_image() {
    let mut member = Member::launch("seed", ImageDescriptor::Absent, false, false);
    let mut seed = member.seed.clone();
    seed.certificate.checker_artifact_sha256 = fixtures::digest(b"wrong checker");
    let request = SeedFrame::Seed {
        request: Box::new(seed),
    };
    member
        .channel
        .get_mut()
        .write_all(&encode_seed_line(&request, FrameDirection::Request).unwrap())
        .unwrap();
    let answer = decode_seed_line(&member.line(), FrameDirection::Response).unwrap();
    assert!(matches!(
        answer,
        SeedFrame::Refused {
            code: SeedRefusal::SeedCertificateInvalid,
            ..
        }
    ));
    member.assert_closed();
    assert!(member.room_names().is_empty());
}

#[test]
fn open_checks_schema_scheme_and_image_identity_before_answering_opened() {
    for variant in 0..3 {
        let mut member = Member::launch("serving", ImageDescriptor::ReadOnly, false, false);
        let mut request = member.open_request();
        let StateFrame::Open {
            schema_id,
            scheme_id,
            start,
            ..
        } = &mut request
        else {
            unreachable!()
        };
        let expected = match variant {
            0 => {
                *schema_id = "retired-schema".into();
                StateRefusalCode::UnsupportedSchema
            }
            1 => {
                *scheme_id = "ste100:wrong".into();
                StateRefusalCode::SchemeMismatch
            }
            _ => {
                start.seed_id = "b".repeat(32);
                StateRefusalCode::OpenFailed
            }
        };
        member.send(&request);
        let answer = decode_state_line(&member.line(), FrameDirection::Response).unwrap();
        assert!(
            matches!(answer, StateFrame::Refused { code, .. } if code == expected),
            "{answer:?}"
        );
        member.assert_closed();
        assert!(member.room_names().is_empty());
    }
}

#[test]
fn fragmented_open_and_coalesced_file_controls_keep_order_and_one_receipt_each() {
    let mut member = Member::launch("serving", ImageDescriptor::ReadOnly, false, false);
    let open = member.open_request();
    let bytes = encode_state_line(&open, FrameDirection::Request).unwrap();
    for chunk in bytes.chunks(7) {
        member.channel.get_mut().write_all(chunk).unwrap();
    }
    assert!(matches!(member.response(&open), StateFrame::Opened { .. }));
    let filing = fixtures::statement(&member.seed, 1, "Read item.");
    let first = StateFrame::File {
        common: member.common(),
        filing: filing.clone(),
    };
    let second = StateFrame::Control {
        common: member.common(),
        frame: StateControl::Held {},
    };
    let mut bytes = encode_state_line(&first, FrameDirection::Request).unwrap();
    bytes.extend(encode_state_line(&second, FrameDirection::Request).unwrap());
    member.channel.get_mut().write_all(&bytes).unwrap();
    assert_filed(member.response(&first), &filing.block_id);
    assert!(matches!(
        member.response(&second),
        StateFrame::ControlAnswer {
            frame: StateControlAnswer::Held {
                count: DecimalU64(2)
            },
            ..
        }
    ));
    assert_eq!(member.observations.len(), 3);
}

#[test]
fn filing_controls_and_metadata_query_keep_content_and_accounting_separate() {
    let mut member = Member::serving();
    let flags = std::fs::read_to_string(format!("/proc/{}/fdinfo/9", member.pid())).unwrap();
    let flags = flags
        .lines()
        .find_map(|line| line.strip_prefix("flags:\t"))
        .unwrap();
    assert_ne!(
        u32::from_str_radix(flags, 8).unwrap() & nix::libc::O_CLOEXEC as u32,
        0
    );
    let mut filing = fixtures::statement(&member.seed, 1, "Read item.");
    filing.raw_block = serde_json::value::RawValue::from_string(
        r#"{ "type":"text", "text":"Read \u0069tem." }"#.into(),
    )
    .unwrap();
    assert_filed(member.file(filing.clone()), &filing.block_id);
    assert_eq!(
        member.control(StateControl::Held {}),
        StateControlAnswer::Held { count: 2.into() }
    );
    assert_eq!(
        member.control(StateControl::Position {}),
        StateControlAnswer::Position(Some(SourcePosition {
            block_id: filing.block_id.clone(),
            turn: filing.source.turn.clone()
        }))
    );
    let StateControlAnswer::Shape(shape) = member.control(StateControl::Shape {}) else {
        panic!()
    };
    assert_eq!(
        shape,
        vec![ShapeRow {
            run: "fixture-run".into(),
            kind: "message.assistant".into(),
            count: 1.into()
        }]
    );
    let StateControlAnswer::Grants { surface } = member.control(StateControl::Grants {}) else {
        panic!()
    };
    assert!(!surface.is_empty());
    let history = entries(member.query(ReadSelector::History {
        noun: "item".into(),
        verb: None,
        condition: None,
    }));
    assert_eq!(history.len(), 2, "the seeded statement remains queryable");
    assert!(matches!(history[0].source, ReadSource::Seed(_)));
    assert_eq!(history[1].raw_block.get(), filing.raw_block.get());
    let expected = member.observations.clone();
    let result = member.query(ReadSelector::Observations {
        run: "fixture-run".into(),
        after: 0.into(),
        through: None,
    });
    let QueryResult::Observations {
        records,
        through,
        member_observation,
        ..
    } = result
    else {
        panic!()
    };
    assert_eq!(
        records, expected,
        "the read must exclude its own observation and later release"
    );
    assert_eq!(through.0, expected.len() as u64);
    assert_eq!(member.identity.as_ref(), Some(&member_observation));
    assert_eq!(
        member.control(StateControl::Held {}),
        StateControlAnswer::Held { count: 2.into() }
    );
    assert!(
        member.room_names().is_empty(),
        "ordinary operations wrote into the member room"
    );
}

#[test]
fn pair_open_and_terminal_file_once_and_retrieve_the_whole_return() {
    let mut member = Member::serving();
    let send = fixtures::statement(&member.seed, 1, "Read item.");
    assert_filed(member.file(send.clone()), &send.block_id);
    let opened = fixtures::opening(&send);
    let opening = fixtures::pair(&member.seed, 2, PairRecord::Started(opened.clone()));
    assert_filed(member.file_pair(opening.clone()), &opening.block_id);
    let QueryResult::OpenPairs { pairs } = member.query(ReadSelector::OpenPairs {}) else {
        panic!()
    };
    assert_eq!(pairs.len(), 1);
    assert_eq!(pairs[0].call_id, opened.call_id);
    let terminal = fixtures::pair(
        &member.seed,
        3,
        PairRecord::Ended(PairEnded {
            call_id: opened.call_id,
            execution: Some(weaver_types::ExchangeId {
                opener: weaver_types::Opener::Harness,
                ordinal: 1,
            }),
            fate: TerminalPairFate::Completed,
            reason: None,
            outcome: Some(weaver_types::ToolOutcome::Result {
                content: "whole\nreturn \u{00e9}".into(),
            }),
            loaded_from: None,
        }),
    );
    assert_filed(member.file_pair(terminal.clone()), &terminal.block_id);
    let history = entries(member.query(ReadSelector::History {
        noun: "item".into(),
        verb: None,
        condition: None,
    }));
    assert_eq!(history.len(), 2, "the seeded statement remains queryable");
    assert!(matches!(history[0].source, ReadSource::Seed(_)));
    assert!(history[0].pairs.is_empty());
    assert_eq!(history[1].pairs.len(), 1);
    assert_eq!(
        history[1].pairs[0]
            .terminal_source
            .as_ref()
            .unwrap()
            .raw_block
            .get(),
        terminal.raw_block.get()
    );
    assert!(
        matches!(member.query(ReadSelector::OpenPairs {}), QueryResult::OpenPairs { pairs } if pairs.is_empty())
    );
    assert_eq!(
        member.control(StateControl::Held {}),
        StateControlAnswer::Held { count: 4.into() }
    );
}

#[test]
fn read_cursor_is_immutable_bounded_and_held_until_explicit_release() {
    let mut member = Member::serving();
    let noun = "e\u{00e9}".repeat(90_000);
    let text = format!("Read {noun}.");
    let filing = fixtures::statement(&member.seed, 1, &text);
    assert_filed(member.file(filing.clone()), &filing.block_id);
    let first = page(member.control(StateControl::Read {
        selector: ReadSelector::Conversation {
            last_turns: 1.into(),
        },
    }));
    assert!(!first.done, "fixture must span more than one page");
    let id = first.snapshot_id;
    let total = first.total_bytes;
    let mut bytes = first.decode_octets(member.max_result_bytes).unwrap();
    assert_eq!(bytes.len(), 524_288);
    read_refused(
        member.control(StateControl::Read {
            selector: ReadSelector::SystemPrompt {},
        }),
        ReadRefusalCode::ReadBusy,
    );
    read_refused(
        member.control(StateControl::Next {
            snapshot_id: id,
            offset: 0.into(),
        }),
        ReadRefusalCode::WrongOffset,
    );
    read_refused(
        member.control(StateControl::Next {
            snapshot_id: (id.0 + 1).into(),
            offset: (bytes.len() as u64).into(),
        }),
        ReadRefusalCode::UnknownSnapshot,
    );
    let later = fixtures::statement(&member.seed, 2, "Read later.");
    assert_filed(member.file(later.clone()), &later.block_id);
    while (bytes.len() as u64) < total.0 {
        let next = page(member.control(StateControl::Next {
            snapshot_id: id,
            offset: (bytes.len() as u64).into(),
        }));
        assert_eq!(next.total_bytes, total);
        assert_eq!(next.offset.0, bytes.len() as u64);
        bytes.extend(next.decode_octets(member.max_result_bytes).unwrap());
    }
    read_refused(
        member.control(StateControl::Next {
            snapshot_id: id,
            offset: total,
        }),
        ReadRefusalCode::WrongOffset,
    );
    let result: QueryResult = decode_strict(std::str::from_utf8(&bytes).unwrap()).unwrap();
    let held = entries(result);
    assert_eq!(
        held.len(),
        1,
        "later filing changed an acquired read snapshot"
    );
    assert_eq!(held[0].text, text);
    assert_eq!(
        member.control(StateControl::Release { snapshot_id: id }),
        StateControlAnswer::Released { snapshot_id: id }
    );
    read_refused(
        member.control(StateControl::Next {
            snapshot_id: id,
            offset: total,
        }),
        ReadRefusalCode::UnknownSnapshot,
    );
    assert_eq!(
        entries(member.query(ReadSelector::Conversation {
            last_turns: 1.into()
        }))
        .len(),
        2
    );
}

#[test]
fn over_limit_read_leaves_no_hidden_cursor_and_never_returns_a_prefix() {
    let mut member = Member::launch("serving", ImageDescriptor::ReadOnly, false, false);
    member.max_result_bytes = 90.into();
    member.open();
    read_refused(
        member.control(StateControl::Read {
            selector: ReadSelector::SystemPrompt {},
        }),
        ReadRefusalCode::ResultTooLarge,
    );
    assert!(
        entries(member.query(ReadSelector::History {
            noun: "absent".into(),
            verb: None,
            condition: None
        }))
        .is_empty()
    );
}

fn snapshot(member: &mut Member, capture: u64) -> (Common, String, Option<SourcePosition>) {
    let common = member.common();
    let answer = member.control_at(
        common.clone(),
        StateControl::Snapshot {
            capture_id: capture.into(),
        },
    );
    let StateControlAnswer::Snapshot {
        capture_id,
        room_name,
        size_bytes,
        position,
    } = answer
    else {
        panic!("{answer:?}");
    };
    assert_eq!(capture_id.0, capture);
    assert_eq!(room_name, format!("capture-{capture}.sqlite.part"));
    assert_eq!(
        std::fs::metadata(member.room().join(&room_name))
            .unwrap()
            .len(),
        size_bytes.0
    );
    (common, room_name, position)
}

fn acknowledge(member: &mut Member, common: Common, capture: u64, part: &str) -> String {
    let bytes = std::fs::read(member.room().join(part)).unwrap();
    let answer = member.control_at(
        common,
        StateControl::Acknowledge {
            capture_id: capture.into(),
            dump_sha256: fixtures::digest(&bytes),
        },
    );
    let StateControlAnswer::Finished {
        capture_id,
        room_name,
    } = answer
    else {
        panic!("{answer:?}");
    };
    assert_eq!(capture_id.0, capture);
    assert_eq!(room_name, format!("capture-{capture}.sqlite"));
    assert!(!member.room().join(part).exists());
    assert_eq!(
        std::fs::read(member.room().join(&room_name)).unwrap(),
        bytes
    );
    room_name
}

#[test]
fn capture_acknowledge_release_and_dump_frontier_match_observed_operations() {
    let mut member = Member::serving();
    let filing = fixtures::statement(&member.seed, 1, "Read item.");
    assert_filed(member.file(filing.clone()), &filing.block_id);
    let before_capture = member.observations.clone();
    let (common, part, position) = snapshot(&mut member, 7);
    let image = std::fs::read(member.room().join(&part)).unwrap();
    let mut start = fixtures::start(&image, &member.seed);
    start.position = position;
    let mut run = fixtures::common(1);
    run.run_id = "reload-fixture".into();
    let loaded = ServingSqlite::adopt(
        &image,
        &run,
        &member.seed.schema_id,
        &member.seed.scheme_id,
        &start,
        fixtures::limits(4_000_000),
    )
    .unwrap();
    let result = loaded
        .query(&ReadSelector::Observations {
            run: "fixture-run".into(),
            after: 0.into(),
            through: None,
        })
        .unwrap();
    assert!(
        matches!(result, QueryResult::Observations { records, .. } if records == before_capture),
        "capture included its own or later operation metadata"
    );
    let finished = acknowledge(&mut member, common, 7, &part);
    assert!(member.room().join(&finished).exists());
    assert_eq!(
        member.control(StateControl::ReleaseCapture {
            capture_id: 7.into()
        }),
        StateControlAnswer::CaptureReleased {
            capture_id: 7.into()
        }
    );
    assert!(member.room_names().is_empty());
    assert_eq!(
        member.control(StateControl::Held {}),
        StateControlAnswer::Held { count: 2.into() }
    );
}

#[test]
fn timeout_abort_in_both_capture_states_deletes_before_ready_and_keeps_exchange_order() {
    for finished in [false, true] {
        let mut member = Member::serving();
        let (common, part, _) = snapshot(&mut member, 11);
        capture_refused(member.control_at(
            common.clone(),
            StateControl::Abort {
                capture_id: 12.into(),
            },
        ));
        let name = if finished {
            acknowledge(&mut member, common.clone(), 11, &part)
        } else {
            part
        };
        assert!(member.room().join(&name).exists());
        assert_eq!(
            member.control_at(
                common,
                StateControl::Abort {
                    capture_id: 11.into()
                }
            ),
            StateControlAnswer::Aborted {
                capture_id: 11.into()
            }
        );
        assert!(
            member.room_names().is_empty(),
            "aborted arrived before retained image deletion"
        );
        let filing = fixtures::statement(&member.seed, 1, "Read item.");
        assert_filed(member.file(filing.clone()), &filing.block_id);
        assert_eq!(
            member.control(StateControl::Held {}),
            StateControlAnswer::Held { count: 2.into() }
        );
    }
}

#[test]
fn fresh_refused_controls_preserve_original_abort_in_both_capture_states() {
    for finished in [false, true] {
        let mut member = Member::serving();
        let (common, part, _) = snapshot(&mut member, 43);
        let name = if finished {
            acknowledge(&mut member, common.clone(), 43, &part)
        } else {
            part
        };
        for control in [
            StateControl::Read {
                selector: ReadSelector::SystemPrompt {},
            },
            StateControl::Snapshot {
                capture_id: 44.into(),
            },
            StateControl::ReleaseCapture {
                capture_id: 44.into(),
            },
        ] {
            capture_refused(member.control(control));
            assert!(member.room().join(&name).exists());
        }
        let next_request = member.next_request;
        capture_refused(member.control_at(
            common.clone(),
            StateControl::Abort {
                capture_id: 44.into(),
            },
        ));
        assert!(member.room().join(&name).exists());
        assert_eq!(
            member.control_at(
                common.clone(),
                StateControl::Abort {
                    capture_id: 43.into(),
                },
            ),
            StateControlAnswer::Aborted {
                capture_id: 43.into(),
            }
        );
        assert!(member.room_names().is_empty());
        assert_eq!(
            member.observations.last().unwrap().observation.request_id,
            common.request_id
        );
        let filing = fixtures::statement(&member.seed, 1, "Read item.");
        assert_filed(member.file(filing.clone()), &filing.block_id);
        assert_eq!(
            member.observations.last().unwrap().observation.request_id.0,
            next_request,
            "original-ID cleanup must neither rewind nor consume fresh IDs"
        );
    }
}

#[test]
fn fresh_refused_control_preserves_original_acknowledgement_and_fresh_release() {
    let mut member = Member::serving();
    let (common, part, _) = snapshot(&mut member, 47);
    capture_refused(member.control(StateControl::Read {
        selector: ReadSelector::SystemPrompt {},
    }));
    let finished = acknowledge(&mut member, common.clone(), 47, &part);
    assert!(member.room().join(&finished).exists());
    assert_eq!(
        member.observations.last().unwrap().observation.request_id,
        common.request_id
    );
    capture_refused(member.control(StateControl::Read {
        selector: ReadSelector::SystemPrompt {},
    }));
    let release_request = member.next_request;
    assert_eq!(
        member.control(StateControl::ReleaseCapture {
            capture_id: 47.into(),
        }),
        StateControlAnswer::CaptureReleased {
            capture_id: 47.into(),
        }
    );
    assert!(member.room_names().is_empty());
    assert_eq!(
        member.observations.last().unwrap().observation.request_id.0,
        release_request
    );
    assert_eq!(
        member.control(StateControl::Held {}),
        StateControlAnswer::Held { count: 1.into() }
    );
}

#[test]
fn eof_in_both_capture_states_deletes_unpublished_room_files() {
    for finished in [false, true] {
        let mut member = Member::serving();
        let (common, part, _) = snapshot(&mut member, 13);
        if finished {
            acknowledge(&mut member, common, 13, &part);
        }
        member.shutdown();
        assert!(member.room_names().is_empty());
    }
}

#[test]
fn filing_during_capture_refuses_then_closes_and_cleans_the_capture() {
    for finished in [false, true] {
        let mut member = Member::serving();
        let (common, part, _) = snapshot(&mut member, 17);
        if finished {
            acknowledge(&mut member, common, 17, &part);
        }
        let filing = fixtures::statement(&member.seed, 1, "Read item.");
        assert!(matches!(
            member.file(filing),
            StateFrame::Refused {
                code: StateRefusalCode::WrongPhase,
                ..
            }
        ));
        member.assert_closed();
        assert!(member.room_names().is_empty());
    }
}

#[test]
fn capture_timeout_cleanup_consumes_a_late_snapshot_before_aborted() {
    let mut member = Member::serving();
    let common = member.common();
    let snapshot = StateFrame::Control {
        common: common.clone(),
        frame: StateControl::Snapshot {
            capture_id: 19.into(),
        },
    };
    let abort = StateFrame::Control {
        common,
        frame: StateControl::Abort {
            capture_id: 19.into(),
        },
    };
    let mut bytes = encode_state_line(&snapshot, FrameDirection::Request).unwrap();
    bytes.extend(encode_state_line(&abort, FrameDirection::Request).unwrap());
    member.channel.get_mut().write_all(&bytes).unwrap();
    assert!(matches!(
        member.response(&snapshot),
        StateFrame::ControlAnswer {
            frame: StateControlAnswer::Snapshot { .. },
            ..
        }
    ));
    assert!(matches!(
        member.response(&abort),
        StateFrame::ControlAnswer {
            frame: StateControlAnswer::Aborted { .. },
            ..
        }
    ));
    assert!(member.room_names().is_empty());
    assert_eq!(
        member.control(StateControl::Held {}),
        StateControlAnswer::Held { count: 1.into() }
    );
}

#[test]
fn duplicate_filing_refuses_closes_and_never_saves_uncertain_state() {
    let mut member = Member::serving();
    let filing = fixtures::statement(&member.seed, 1, "Read item.");
    assert_filed(member.file(filing.clone()), &filing.block_id);
    assert!(matches!(
        member.file(filing),
        StateFrame::Refused {
            code: StateRefusalCode::DuplicateSource,
            ..
        }
    ));
    member.assert_closed();
    assert!(member.room_names().is_empty());
}

#[test]
fn refused_capture_retains_only_its_correlated_cleanup_until_the_next_exchange() {
    let mut member = Member::serving();
    let capture_id = DecimalU64(23);
    let collision = member.room().join("capture-23.sqlite.part");
    std::fs::write(&collision, b"an unconfirmed remnant").unwrap();
    let common = member.common();
    let answer = member.control_at(common.clone(), StateControl::Snapshot { capture_id });
    assert!(
        matches!(
            answer,
            StateControlAnswer::ControlRefused {
                code: ControlRefusalCode::Capture(CaptureRefusalCode::SnapshotFailed),
                ..
            }
        ),
        "{answer:?}"
    );
    assert!(
        !collision.exists(),
        "a refused capture retained a room file"
    );
    assert_eq!(
        member.control_at(common, StateControl::Abort { capture_id }),
        StateControlAnswer::Aborted { capture_id }
    );
    assert_eq!(
        member.control(StateControl::Held {}),
        StateControlAnswer::Held { count: 1.into() }
    );
    assert!(member.room_names().is_empty());
}

#[test]
fn refused_capture_cannot_be_acknowledged_through_its_retained_cleanup_identity() {
    let mut member = Member::serving();
    std::fs::write(member.room().join("capture-41.sqlite.part"), b"collision").unwrap();
    let common = member.common();
    let answer = member.control_at(
        common.clone(),
        StateControl::Snapshot {
            capture_id: 41.into(),
        },
    );
    assert!(matches!(
        answer,
        StateControlAnswer::ControlRefused {
            code: ControlRefusalCode::Capture(CaptureRefusalCode::SnapshotFailed),
            ..
        }
    ));
    member.send(&StateFrame::Control {
        common,
        frame: StateControl::Acknowledge {
            capture_id: 41.into(),
            dump_sha256: fixtures::digest(b"no captured image"),
        },
    });
    member.assert_closed();
    assert!(member.room_names().is_empty());
}

#[test]
fn fresh_request_retires_a_refused_capture_cleanup_identity() {
    let mut member = Member::serving();
    std::fs::write(member.room().join("capture-53.sqlite.part"), b"collision").unwrap();
    let common = member.common();
    assert!(matches!(
        member.control_at(
            common.clone(),
            StateControl::Snapshot {
                capture_id: 53.into(),
            },
        ),
        StateControlAnswer::ControlRefused {
            code: ControlRefusalCode::Capture(CaptureRefusalCode::SnapshotFailed),
            ..
        }
    ));
    assert_eq!(
        member.control(StateControl::Held {}),
        StateControlAnswer::Held { count: 1.into() }
    );
    member.send(&StateFrame::Control {
        common,
        frame: StateControl::Abort {
            capture_id: 53.into(),
        },
    });
    member.assert_closed();
    assert!(member.room_names().is_empty());
}

#[test]
fn capture_blocks_reads_and_second_open_in_both_held_states() {
    for finished in [false, true] {
        let mut member = Member::serving();
        let (common, part, _) = snapshot(&mut member, 29);
        if finished {
            acknowledge(&mut member, common, 29, &part);
        }
        capture_refused(member.control(StateControl::Read {
            selector: ReadSelector::SystemPrompt {},
        }));
        assert_eq!(
            member.room_names().len(),
            1,
            "invalid control released its capture"
        );
        let mut request = member.open_request();
        let StateFrame::Open { common, .. } = &mut request else {
            unreachable!()
        };
        common.request_id = 1.into();
        member.send(&request);
        member.assert_closed();
        assert!(member.room_names().is_empty());
    }
}

#[test]
fn failure_to_delete_a_capture_is_component_failure_never_a_successful_cleanup() {
    let mut member = Member::serving();
    let (common, part, _) = snapshot(&mut member, 31);
    let name = member.room().join(part);
    std::fs::remove_file(&name).unwrap();
    std::fs::create_dir(&name).unwrap();
    let abort = StateFrame::Control {
        common,
        frame: StateControl::Abort {
            capture_id: 31.into(),
        },
    };
    member.send(&abort);
    member.assert_closed();
    assert!(
        name.is_dir(),
        "the fixture must prove the cleanup was impossible"
    );
    assert!(!member.log().is_empty());
}

#[test]
fn only_acknowledge_and_abort_may_continue_the_original_capture_exchange() {
    for invalid_read in [true, false] {
        let mut member = Member::serving();
        let (common, part, _) = snapshot(&mut member, 37);
        let frame = if invalid_read {
            StateControl::Read {
                selector: ReadSelector::SystemPrompt {},
            }
        } else {
            acknowledge(&mut member, common.clone(), 37, &part);
            StateControl::ReleaseCapture {
                capture_id: 37.into(),
            }
        };
        // Reads and successful capture release require fresh request IDs.
        member.send(&StateFrame::Control { common, frame });
        member.assert_closed();
        assert!(member.room_names().is_empty());
    }
}

#[test]
fn unrecognized_sqlite_image_refuses_without_legacy_migration() {
    let seed = fixtures::seed();
    let image = seed_image(&seed).unwrap().into_bytes();
    let mut wrong = image.clone();
    wrong[..16].copy_from_slice(b"not SQLite data!");
    let mut member = Member::launch_image(
        "serving",
        ImageDescriptor::ReadOnly,
        false,
        false,
        seed,
        &wrong,
    );
    let request = member.open_request();
    member.send(&request);
    let answer = decode_state_line(&member.line(), FrameDirection::Response).unwrap();
    assert!(matches!(
        answer,
        StateFrame::Refused {
            code: StateRefusalCode::OpenFailed | StateRefusalCode::UnsupportedSchema,
            ..
        }
    ));
    member.assert_closed();
    assert!(member.room_names().is_empty());
}
