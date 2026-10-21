//! Test-only marker at the start of the real operation hint parser.

use std::cell::Cell;

use super::super::*;

thread_local! {
    static ENABLED: Cell<bool> = const { Cell::new(false) };
    static PEEK_START: Cell<Option<DecimalU64>> = const { Cell::new(None) };
}

pub(crate) fn enable() {
    ENABLED.set(true);
    PEEK_START.set(None);
}

pub(crate) fn before_peek() {
    if ENABLED.get() {
        PEEK_START.set(Some(monotonic_ns().unwrap()));
    }
}

pub(crate) fn verify(observation: &StateOperationObservation) {
    if !ENABLED.get() || observation.operation == StateOperation::Snapshot {
        return;
    }
    let peek = PEEK_START.get().expect("the real hint parser ran");
    assert!(
        observation.began_ns <= peek,
        "operation interval omitted preparsing: {:?} > {:?}",
        observation.began_ns,
        peek
    );
    let validate = observation
        .phases
        .iter()
        .find(|phase| phase.name == PhaseName::Validate)
        .unwrap();
    assert!(peek <= validate.began_ns);
}

#[test]
fn an_ordinary_operation_retains_the_clock_from_before_the_hint_parser() {
    let request = StateFrame::Control {
        common: Common {
            session_id: "session".into(),
            run_id: "run".into(),
            request_id: 2.into(),
        },
        frame: StateControl::Held {},
    };
    let bytes = encode_state_line(&request, FrameDirection::Request).unwrap();
    enable();
    let (sample, _) = sampled_request(&bytes, 2).unwrap();
    let (observation, _) = complete(
        Reply::Control(StateControlAnswer::Held { count: 1.into() }),
        sample,
    )
    .unwrap();
    verify(&observation);
    ENABLED.set(false);
}

#[test]
fn snapshot_cannot_borrow_a_clock_from_before_its_resource_sample() {
    let start = monotonic_ns().unwrap();
    assert!(
        OperationSample::begin_at(1.into(), 1.into(), StateOperation::Snapshot, start).is_err()
    );
}
