use super::*;

pub(super) mod process;

pub(super) mod timing;

#[test]
fn observed_reply_bytes_match_the_shared_canonical_codec() {
    let common = Common {
        session_id: "session \"quoted\"\n".into(),
        run_id: "run".into(),
        request_id: u64::MAX.into(),
    };
    let block = SourceBlockId {
        run: "run".into(),
        sequence: u64::MAX.into(),
        block: u64::MAX.into(),
    };
    let replies = [
        (
            StateOperation::File,
            Reply::Filed {
                block_id: block.clone(),
            },
        ),
        (
            StateOperation::File,
            Reply::Refused {
                code: StateRefusalCode::WrongRun,
                detail: "refused \"quoted\"\ntext".into(),
                block_id: Some(block),
            },
        ),
        (
            StateOperation::Next,
            Reply::Control(StateControlAnswer::ReadPage(ReadPage {
                snapshot_id: 9.into(),
                offset: 0.into(),
                total_bytes: 3.into(),
                octets: "YWJj".into(),
                done: true,
            })),
        ),
        (
            StateOperation::Position,
            Reply::Control(StateControlAnswer::Position(None)),
        ),
    ];
    for (operation, reply) in replies {
        let op = reply.op();
        let mut sample =
            OperationSample::begin(u64::MAX.into(), common.request_id, operation).unwrap();
        sample.phase_begin(PhaseName::Validate).unwrap();
        sample.phase_end(PhaseName::Validate).unwrap();
        let (observation, payload) = complete(reply, sample).unwrap();
        let bytes = wrap(&common, op, &payload, &observation).unwrap();
        let decoded = decode_state_line(&bytes, FrameDirection::Response).unwrap();
        assert_eq!(
            bytes,
            encode_state_line(&decoded, FrameDirection::Response).unwrap()
        );
        assert_eq!(bytes.iter().filter(|b| **b == b'\n').count(), 1);
    }
}

#[test]
fn page_budget_reserves_actual_wrapper_and_worst_case_metadata() {
    let common = Common {
        session_id: "session".repeat(100),
        run_id: "run".repeat(100),
        request_id: u64::MAX.into(),
    };
    let budget = page_budget(&common).unwrap();
    let mut pages = Pages::default();
    let page = pages
        .start(vec![b'x'; 524_288], 524_288.into(), budget)
        .unwrap();
    let payload = encode_payload(&Reply::Control(StateControlAnswer::ReadPage(page))).unwrap();
    assert!(
        payload.len() + wrapper_reserve(&common, "control_answer").unwrap()
            <= STATE_ANSWER_LINE_BOUND
    );
    let huge = Common {
        session_id: "s".repeat(STATE_ANSWER_LINE_BOUND),
        ..common
    };
    assert!(page_budget(&huge).is_err());
}
