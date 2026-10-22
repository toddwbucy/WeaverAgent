use super::*;
use std::io::Write;
use std::os::unix::net::UnixStream;

fn sample(operation: StateOperation) -> OperationSample {
    let mut sample = OperationSample::begin(2.into(), 2.into(), operation).unwrap();
    sample.phase_begin(PhaseName::Validate).unwrap();
    sample.phase_end(PhaseName::Validate).unwrap();
    sample.held = BufferBytes::default();
    sample
}

#[test]
fn completion_counts_failed_encoding_prefix_and_samples_after_page_cleanup() {
    for initial_read in [false, true] {
        let common = Common {
            session_id: "session".into(),
            run_id: "run".into(),
            request_id: 2.into(),
        };
        let mut pages = Pages::default();
        let mut raw = Vec::with_capacity(4096);
        raw.extend_from_slice(b"retained bytes");
        pages.start(raw, 100.into(), 100).unwrap();
        let payload = Reply::Control(StateControlAnswer::Position(Some(SourcePosition {
            block_id: SourceBlockId {
                run: "run".into(),
                sequence: 1.into(),
                block: 0.into(),
            },
            turn: Some("t".repeat(STATE_ANSWER_LINE_BOUND)),
        })));
        let (observation, encoded) = complete(
            payload,
            sample(StateOperation::Position),
            Some(CompletionBounds {
                common: &common,
                pages: &mut pages,
                initial_read,
            }),
        )
        .unwrap();
        let prefix = br#"{"frame":{"position":{"block_id":{"run":"run","sequence":"1","block":"0"},"turn":""#;
        assert_eq!(
            observation.resource.copied_bytes,
            Some((prefix.len() as u64 + encoded.len() as u64).into())
        );
        assert_eq!(
            observation.resource.held_bytes,
            Some((encoded.len() as u64 + if initial_read { 0 } else { 14 }).into())
        );
        assert_eq!(pages.busy(), !initial_read);
    }
}

#[test]
fn next_counts_its_new_page_once_and_keeps_the_full_snapshot_until_release() {
    let mut fx = timing::Fixture::new();
    let mut raw = Vec::with_capacity(4096);
    raw.extend_from_slice(b"abcdefghij");
    let first = fx.session.pages.start(raw, 10.into(), 4).unwrap();
    fx.session.pages.publish_page();
    let common = process::fixtures::common(2);
    let request = StateFrame::Control {
        common: common.clone(),
        frame: StateControl::Next {
            snapshot_id: first.snapshot_id,
            offset: 3.into(),
        },
    };
    let line = encode_state_line(&request, FrameDirection::Request).unwrap();
    let (mut sample, request) = sampled_request(&line, 2).unwrap();
    let raw_fields: BTreeMap<String, &RawValue> = serde_json::from_slice(&line).unwrap();
    let decoded: usize = raw_fields.values().map(|v| v.get().len()).sum();
    let (payload, initial_read) = fx.session.dispatch(request, &fx.room, &mut sample).unwrap();
    let Reply::Control(StateControlAnswer::ReadPage(page)) = &payload else {
        panic!("next page");
    };
    assert_eq!(page.octets, "ZGVmZ2hpag==");
    assert!(page.done);
    assert_eq!(page.total_bytes.0, 10);
    sample.held = BufferBytes::default();
    let (observation, encoded) = complete(
        payload,
        sample,
        Some(CompletionBounds {
            common: &common,
            pages: &mut fx.session.pages,
            initial_read,
        }),
    )
    .unwrap();
    assert_eq!(
        observation.resource.copied_bytes,
        Some((decoded as u64 + 12 + encoded.len() as u64).into())
    );
    assert_eq!(
        observation.resource.held_bytes,
        Some((10 + encoded.len() as u64).into())
    );
    fx.session.pages.publish_page();
    fx.session.pages.release(first.snapshot_id).unwrap();
    assert_eq!(fx.session.pages.held_bytes(), 0);
}

#[test]
fn request_accounting_includes_actual_read_ahead_and_ignores_spare_capacity() {
    let common = process::fixtures::common(2);
    let one = encode_state_line(
        &StateFrame::Control {
            common: common.clone(),
            frame: StateControl::Held {},
        },
        FrameDirection::Request,
    )
    .unwrap();
    let two = encode_state_line(
        &StateFrame::Control {
            common: process::fixtures::common(3),
            frame: StateControl::Held {},
        },
        FrameDirection::Request,
    )
    .unwrap();
    let (ours, mut peer) = UnixStream::pair().unwrap();
    let mut channel = Channel::new(ours).unwrap();
    let mut both = one.clone();
    both.extend_from_slice(&two);
    peer.write_all(&both).unwrap();
    let mut line = channel.receive().unwrap().unwrap();
    assert_eq!(line, one);
    assert_eq!(channel.buffered_bytes(), two.len());
    line.reserve(4096);
    let mut sample = sample(StateOperation::Held);
    sample.held = held_request_bytes(&line, &channel, &common, None, None);
    let (observation, encoded) = complete(
        Reply::Control(StateControlAnswer::Held { count: 1.into() }),
        sample,
        None,
    )
    .unwrap();
    assert_eq!(
        observation.resource.held_bytes,
        Some((one.len() as u64 + two.len() as u64 + 26 + encoded.len() as u64).into())
    );
    assert_eq!(channel.receive().unwrap().unwrap(), two);
    assert_eq!(channel.buffered_bytes(), 0);
}

#[test]
fn byte_counter_overflow_is_reported_without_changing_the_valid_answer() {
    let mut sample = sample(StateOperation::Held);
    sample.copied = BufferBytes::measured(u64::MAX);
    sample.held = BufferBytes::measured(u64::MAX);
    let (observation, encoded) = complete(
        Reply::Control(StateControlAnswer::Held { count: 1.into() }),
        sample,
        None,
    )
    .unwrap();
    assert_eq!(encoded, br#"{"frame":{"held":{"count":"1"}}}"#);
    assert!(observation.resource.copied_bytes.is_none());
    assert!(observation.resource.held_bytes.is_none());
    for field in [ResourceField::CopiedBytes, ResourceField::HeldBytes] {
        assert!(
            observation
                .resource
                .unavailable
                .contains(&ResourceUnavailable {
                    field,
                    reason: ObservationUnavailable::Overflow
                })
        );
    }
}
