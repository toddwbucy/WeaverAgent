//! Real run() observations. Expected held owners come from peer-visible bytes,
//! read/capture identities and their lifetimes, not the production sum helper.

use super::*;

fn payload_len(wire: &[u8], names: &[&str]) -> u64 {
    let fields: BTreeMap<String, &RawValue> = serde_json::from_slice(wire).unwrap();
    // Literal braces, keys, colons and commas plus unchanged on-wire values.
    (2 + names
        .iter()
        .map(|name| name.len() + 3 + fields[*name].get().len())
        .sum::<usize>()
        + names.len()
        - 1) as u64
}

fn header_len(request: &StateFrame) -> u64 {
    let common = request.common();
    format!(
        "{{\"protocol\":\"state-flow/1\",\"session_id\":\"{}\",\"run_id\":\"{}\",\"request_id\":\"{}\",\"op\":\"control_answer\"}}",
        common.session_id, common.run_id, common.request_id.0
    ).len() as u64
}

fn raw_copies(request_bytes: &[u8]) -> u64 {
    let fields: BTreeMap<String, &RawValue> = serde_json::from_slice(request_bytes).unwrap();
    fields.values().map(|raw| raw.get().len() as u64).sum()
}

pub(super) fn check_open(request: &StateFrame, answer: &StateFrame, wire: &[u8]) {
    let request_bytes = encode_state_line(request, FrameDirection::Request).unwrap();
    let StateFrame::Opened {
        read_limits,
        member_observation,
        observation,
        ..
    } = answer
    else {
        panic!("opened");
    };
    let payload = payload_len(
        wire,
        &[
            "schema_id",
            "scheme_id",
            "start",
            "read_limits",
            "member_observation",
        ],
    );
    assert_eq!(
        observation.resource.held_bytes,
        Some(
            (request_bytes.len() as u64
                + payload
                + request.common().session_id.len() as u64
                + request.common().run_id.len() as u64
                + read_limits.measurement_ref.len() as u64
                + member_observation.instrument_version.len() as u64)
                .into()
        )
    );
    assert!(
        observation.resource.copied_bytes.is_none(),
        "adoption copies remain incomplete"
    );
}

struct Answer {
    frame: StateControlAnswer,
    observation: StateOperationObservation,
    request: StateFrame,
    request_bytes: Vec<u8>,
    payload_bytes: u64,
}

fn exchange(
    peer: &mut BufReader<UnixStream>,
    request_id: u64,
    operation_id: u64,
    frame: StateControl,
) -> Answer {
    let request = StateFrame::Control {
        common: fixtures::common(request_id),
        frame,
    };
    let request_bytes = encode_state_line(&request, FrameDirection::Request).unwrap();
    peer.get_mut().write_all(&request_bytes).unwrap();
    let mut wire = Vec::new();
    assert!(peer.read_until(b'\n', &mut wire).unwrap() > 0);
    let answer = decode_state_line(&wire, FrameDirection::Response).unwrap();
    answer
        .validate_response_to(&request, operation_id.into())
        .unwrap();
    let StateFrame::ControlAnswer {
        frame, observation, ..
    } = answer
    else {
        panic!("control answer");
    };
    Answer {
        frame,
        observation,
        request,
        request_bytes,
        payload_bytes: payload_len(&wire, &["frame"]),
    }
}

impl Answer {
    fn check(&self, read: u64, capture: u64, extra_copy: Option<u64>) {
        let common = self.request.common();
        assert_eq!(
            self.observation.resource.held_bytes,
            Some(
                (self.request_bytes.len() as u64
                    + common.session_id.len() as u64
                    + common.run_id.len() as u64
                    + read
                    + capture
                    + self.payload_bytes)
                    .into()
            ),
            "wrong live owners for {:?}",
            self.request.request_operation()
        );
        if let Some(extra) = extra_copy {
            assert_eq!(
                self.observation.resource.copied_bytes,
                Some(
                    (raw_copies(&self.request_bytes)
                        + header_len(&self.request)
                        + self.payload_bytes
                        + extra)
                        .into()
                ),
                "wrong copies for {:?}",
                self.request.request_operation()
            );
        } else {
            assert!(self.observation.resource.copied_bytes.is_none());
            assert!(
                self.observation
                    .resource
                    .unavailable
                    .contains(&ResourceUnavailable {
                        field: ResourceField::CopiedBytes,
                        reason: ObservationUnavailable::Uninstrumented,
                    })
            );
        }
    }
}

fn finish(fixture: &mut Fixture, peer: &mut BufReader<UnixStream>) {
    peer.get_ref().shutdown(std::net::Shutdown::Write).unwrap();
    assert_eq!(peer.read(&mut [0]).unwrap(), 0);
    fixture.finish();
}

#[test]
fn capture_accounting_counts_one_image_copy_and_only_live_buffers_through_cleanup() {
    let (mut fixture, mut peer, _) = launch("accounting");
    let read = exchange(
        &mut peer,
        2,
        2,
        StateControl::Read {
            selector: ReadSelector::SystemPrompt {},
        },
    );
    let StateControlAnswer::ReadPage(page) = &read.frame else {
        panic!("read page");
    };
    assert!(
        page.done,
        "even the final page retains the full result until release"
    );
    let (snapshot, total) = (page.snapshot_id, page.total_bytes.0);
    read.check(total, 0, None);

    let snapshot_answer = exchange(
        &mut peer,
        3,
        3,
        StateControl::Snapshot {
            capture_id: 7.into(),
        },
    );
    let StateControlAnswer::Snapshot {
        room_name,
        size_bytes,
        ..
    } = &snapshot_answer.frame
    else {
        panic!("snapshot");
    };
    let image = size_bytes.0;
    assert!(image > 4096);
    let held_name = room_name.len() as u64;
    snapshot_answer.check(total, held_name, Some(image));
    assert_eq!(
        std::fs::metadata(fixture.root.join("room").join(room_name))
            .unwrap()
            .len(),
        image
    );

    let refused = exchange(
        &mut peer,
        4,
        4,
        StateControl::Snapshot {
            capture_id: 8.into(),
        },
    );
    assert!(matches!(
        refused.frame,
        StateControlAnswer::ControlRefused { .. }
    ));
    refused.check(total, held_name, Some(0));
    assert_eq!(
        refused.observation.disk.unavailable_reason,
        Some(ObservationUnavailable::NotSampled)
    );

    let finished = exchange(
        &mut peer,
        3,
        5,
        StateControl::Acknowledge {
            capture_id: 7.into(),
            dump_sha256: fixtures::digest(b"admin hashes the image"),
        },
    );
    let StateControlAnswer::Finished { room_name, .. } = &finished.frame else {
        panic!("finished");
    };
    finished.check(total, room_name.len() as u64 + 64, Some(0));
    let released_capture = exchange(
        &mut peer,
        5,
        6,
        StateControl::ReleaseCapture {
            capture_id: 7.into(),
        },
    );
    released_capture.check(total, 0, Some(0));
    assert!(
        std::fs::read_dir(fixture.root.join("room"))
            .unwrap()
            .next()
            .is_none()
    );
    let released = exchange(
        &mut peer,
        6,
        7,
        StateControl::Release {
            snapshot_id: snapshot,
        },
    );
    released.check(0, 0, Some(0));
    finish(&mut fixture, &mut peer);
}

#[test]
fn capture_preflight_and_failed_write_do_not_invent_held_image_or_lose_copies() {
    let (mut fixture, mut peer, _) = launch("accounting");
    let zero = exchange(
        &mut peer,
        2,
        2,
        StateControl::Snapshot {
            capture_id: 0.into(),
        },
    );
    assert!(matches!(
        zero.frame,
        StateControlAnswer::ControlRefused { .. }
    ));
    zero.check(0, 0, Some(0));
    let snapshot = exchange(
        &mut peer,
        3,
        3,
        StateControl::Snapshot {
            capture_id: 7.into(),
        },
    );
    let StateControlAnswer::Snapshot { room_name, .. } = &snapshot.frame else {
        panic!("snapshot");
    };
    let image_path = fixture.root.join("room").join(room_name);
    let image_before = std::fs::metadata(&image_path).unwrap().len();
    let aborted = exchange(
        &mut peer,
        3,
        4,
        StateControl::Abort {
            capture_id: 7.into(),
        },
    );
    aborted.check(0, 0, Some(0));
    assert!(!image_path.exists());
    std::fs::write(fixture.root.join("room/capture-8.sqlite.part"), b"stale").unwrap();
    let refused = exchange(
        &mut peer,
        4,
        5,
        StateControl::Snapshot {
            capture_id: 8.into(),
        },
    );
    assert!(matches!(
        refused.frame,
        StateControlAnswer::ControlRefused { .. }
    ));
    // Metadata may grow the SQLite image since the first snapshot. Its copy must
    // still be charged although the create_new collision refuses the room write.
    assert_eq!(
        refused.observation.resource.held_bytes,
        Some((refused.request_bytes.len() as u64 + 26 + refused.payload_bytes).into())
    );
    let ordinary =
        raw_copies(&refused.request_bytes) + header_len(&refused.request) + refused.payload_bytes;
    assert!(refused.observation.resource.copied_bytes.unwrap().0 >= ordinary + image_before);
    assert!(
        std::fs::read_dir(fixture.root.join("room"))
            .unwrap()
            .next()
            .is_none()
    );
    let aborted = exchange(
        &mut peer,
        4,
        6,
        StateControl::Abort {
            capture_id: 8.into(),
        },
    );
    aborted.check(0, 0, Some(0));
    finish(&mut fixture, &mut peer);
}

#[test]
fn metadata_controls_have_complete_copy_totals_and_filing_keeps_explicit_missing_coverage() {
    let (mut fixture, mut peer, seed) = launch("accounting");
    let mut filing = fixtures::statement(&seed, 1, "Read caf\u{e9}.");
    filing.raw_block =
        RawValue::from_string(r#"{ "type":"text", "text":"Read caf\u00e9." }"#.into()).unwrap();
    let file = StateFrame::File {
        common: fixtures::common(2),
        filing,
    };
    let bytes = encode_state_line(&file, FrameDirection::Request).unwrap();
    peer.get_mut().write_all(&bytes).unwrap();
    let mut wire = Vec::new();
    assert!(peer.read_until(b'\n', &mut wire).unwrap() > 0);
    let answer = decode_state_line(&wire, FrameDirection::Response).unwrap();
    answer.validate_response_to(&file, 2.into()).unwrap();
    let StateFrame::Filed {
        observation,
        block_id,
        ..
    } = answer
    else {
        panic!("filed");
    };
    assert!(observation.resource.copied_bytes.is_none());
    assert!(
        observation
            .resource
            .unavailable
            .contains(&ResourceUnavailable {
                field: ResourceField::CopiedBytes,
                reason: ObservationUnavailable::Uninstrumented,
            })
    );
    assert_eq!(
        observation.resource.held_bytes,
        Some(
            (bytes.len() as u64
                + 26
                + block_id.run.len() as u64
                + payload_len(&wire, &["block_id"]))
            .into()
        )
    );

    let shape = exchange(&mut peer, 3, 3, StateControl::Shape {});
    assert!(matches!(&shape.frame, StateControlAnswer::Shape(rows) if rows.len() == 1));
    let shape_buffer = br#"[{"run":"fixture-run","kind":"message.assistant","count":"1"}]"#;
    shape.check(0, 0, Some(shape_buffer.len() as u64));
    let held = exchange(&mut peer, 4, 4, StateControl::Held {});
    held.check(0, 0, Some(0));
    let position = exchange(&mut peer, 5, 5, StateControl::Position {});
    position.check(0, 0, Some(0));
    let grants = exchange(&mut peer, 6, 6, StateControl::Grants {});
    grants.check(0, 0, Some(0));
    finish(&mut fixture, &mut peer);
}
