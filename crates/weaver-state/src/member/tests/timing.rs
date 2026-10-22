//! Test-only marker at the start of the real operation hint parser.

use std::cell::{Cell, RefCell};

use super::process::fixtures;
use crate::member::observation::SnapshotFacts;

use super::super::*;

thread_local! {
    static ENABLED: Cell<bool> = const { Cell::new(false) };
    static PEEK_START: Cell<Option<DecimalU64>> = const { Cell::new(None) };
    static EVENTS: RefCell<Vec<(&'static str, DecimalU64)>> = const { RefCell::new(Vec::new()) };
    static SAMPLE_COUNT: Cell<u64> = const { Cell::new(0) };
    static UNAVAILABLE: Cell<bool> = const { Cell::new(false) };
    static IMAGE_FAILURE: Cell<bool> = const { Cell::new(false) };
}

pub(crate) fn enable() {
    ENABLED.set(true);
    PEEK_START.set(None);
    EVENTS.with(|events| events.borrow_mut().clear());
    SAMPLE_COUNT.set(0);
    UNAVAILABLE.set(false);
    IMAGE_FAILURE.set(false);
}

pub(in crate::member) fn resource_sample_count() -> u64 {
    SAMPLE_COUNT.get()
}

pub(in crate::member) fn disable() {
    ENABLED.set(false);
}

pub(crate) fn before_peek() {
    if ENABLED.get() {
        PEEK_START.set(Some(monotonic_ns().unwrap()));
    }
}

pub(crate) fn verify(observation: &StateOperationObservation) {
    if !ENABLED.get() {
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
    verify_payload_encoding(observation);
}

fn verify_payload_encoding(observation: &StateOperationObservation) {
    let encode = observation
        .phases
        .iter()
        .find(|phase| phase.name == PhaseName::Encode)
        .expect("answer payload has an encode phase");
    EVENTS.with(|events| {
        let events = events.borrow();
        let last = |name| {
            events
                .iter()
                .rev()
                .find(|(event, _)| *event == name)
                .expect("actual payload encoder marker")
                .1
        };
        let began = last("encode");
        let encoded = last("payload_encoded");
        assert!(
            encode.began_ns <= began,
            "payload encoding began before its phase"
        );
        assert!(
            began <= encoded && encoded <= encode.ended_ns,
            "payload encoding is outside its phase"
        );
        assert!(
            encoded <= observation.ended_ns,
            "observation ended before the payload was encoded"
        );
    });
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
        None,
    )
    .unwrap();
    verify(&observation);
    ENABLED.set(false);
}

pub(in crate::member) fn mark(event: &'static str) {
    if ENABLED.get() {
        EVENTS.with(|events| events.borrow_mut().push((event, monotonic_ns().unwrap())));
    }
}

pub(in crate::member) fn observation_end(ended_ns: DecimalU64) {
    if ENABLED.get() {
        EVENTS.with(|events| events.borrow_mut().push(("ended", ended_ns)));
    }
}

pub(in crate::member) fn sample_resources() -> Option<SnapshotFacts> {
    if !ENABLED.get() {
        return None;
    }
    let count = SAMPLE_COUNT.get() + 1;
    SAMPLE_COUNT.set(count);
    mark(if count == 1 { "before" } else { "after" });
    Some(if UNAVAILABLE.get() {
        SnapshotFacts {
            cpu: Err(ObservationUnavailable::Denied),
            enabled: Err(ObservationUnavailable::ReadFailed),
            disk: Err(ObservationUnavailable::Unsupported),
        }
    } else {
        SnapshotFacts {
            cpu: Ok(10 * count),
            enabled: Ok(false),
            disk: Ok(DiskSample {
                major_faults: count.into(),
                read_bytes: count.into(),
                write_bytes: count.into(),
                blkio_delay_ticks: count.into(),
                ticks_per_second: 100.into(),
            }),
        }
    })
}

pub(in crate::member) fn image_result(
    result: Result<Vec<u8>, weaver_state::engine::serving_sqlite::CaptureFault>,
) -> Result<Vec<u8>, weaver_state::engine::serving_sqlite::CaptureFault> {
    // Exercise the recoverable serialization-result path after running the real
    // RAM serializer. No environment-controlled hook exists in the member binary.
    if IMAGE_FAILURE.replace(false) {
        return Err(weaver_state::engine::serving_sqlite::CaptureFault {
            code: CaptureRefusalCode::SnapshotFailed,
            detail: "injected image serialization failure".into(),
        });
    }
    result
}

struct Fixture {
    root: std::path::PathBuf,
    room: Room,
    session: Session,
}

impl Fixture {
    fn new() -> Self {
        use std::sync::atomic::{AtomicU64, Ordering};
        static NEXT: AtomicU64 = AtomicU64::new(0);
        let root = std::env::temp_dir().join(format!(
            "state-snapshot-sampling-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        std::fs::create_dir(&root).unwrap();
        let room = Room::open(&root).unwrap();
        let seed = fixtures::seed();
        let image = weaver_state::engine::serving_sqlite::seed_image(&seed).unwrap();
        let common = fixtures::common(1);
        let read_limits = fixtures::limits(1_000_000);
        let store = ServingSqlite::adopt(
            image.as_bytes(),
            &common,
            &seed.schema_id,
            &seed.scheme_id,
            &fixtures::start(image.as_bytes(), &seed),
            read_limits.clone(),
        )
        .unwrap();
        Self {
            root,
            room,
            session: Session {
                store,
                common,
                last_request: 1,
                last_operation: 1,
                pages: Pages::default(),
                capture: Capture::default(),
                read_limits,
            },
        }
    }

    fn snapshot(
        &mut self,
        request: u64,
    ) -> (StateControlAnswer, StateOperationObservation, Vec<u8>) {
        self.snapshot_with_capture_id(request, 7)
    }

    fn snapshot_with_capture_id(
        &mut self,
        request: u64,
        capture_id: u64,
    ) -> (StateControlAnswer, StateOperationObservation, Vec<u8>) {
        let request = StateFrame::Control {
            common: fixtures::common(request),
            frame: StateControl::Snapshot {
                capture_id: capture_id.into(),
            },
        };
        let bytes = encode_state_line(&request, FrameDirection::Request).unwrap();
        let (mut sample, request) = sampled_request(&bytes, request.common().request_id.0).unwrap();
        self.session.check_request(&request).unwrap();
        let common = request.common().clone();
        let (reply, initial_read) = self
            .session
            .dispatch(request, &self.room, &mut sample)
            .unwrap();
        let Reply::Control(answer) = reply else {
            panic!("control reply")
        };
        let (observation, encoded) = complete(
            Reply::Control(answer.clone()),
            sample,
            Some(CompletionBounds {
                common: &common,
                pages: &mut self.session.pages,
                initial_read,
            }),
        )
        .unwrap();
        verify(&observation);
        (answer, observation, encoded)
    }
}

impl Drop for Fixture {
    fn drop(&mut self) {
        ENABLED.set(false);
        let _ = std::fs::remove_dir_all(&self.root);
    }
}

fn recorded_events() -> Vec<&'static str> {
    EVENTS.with(|events| events.borrow().iter().map(|(event, _)| *event).collect())
}

fn assert_capture_interval(observation: &StateOperationObservation) {
    let timestamp = |event| {
        EVENTS.with(|events| {
            events
                .borrow()
                .iter()
                .find(|(name, _)| *name == event)
                .unwrap()
                .1
        })
    };
    let phase = |name| {
        observation
            .phases
            .iter()
            .find(|phase| phase.name == name)
            .unwrap()
    };
    assert!(phase(PhaseName::Validate).ended_ns <= timestamp("before"));
    assert!(timestamp("before") <= phase(PhaseName::Capture).began_ns);
    assert!(phase(PhaseName::Capture).ended_ns <= timestamp("after"));
    if let Some(filesystem) = observation
        .phases
        .iter()
        .find(|phase| phase.name == PhaseName::Filesystem)
    {
        assert!(filesystem.ended_ns <= timestamp("after"));
    }
    assert!(timestamp("after") <= phase(PhaseName::Encode).began_ns);
    assert!(timestamp("payload_encoded") <= observation.ended_ns);
    assert_eq!(timestamp("ended"), observation.ended_ns);
    assert_eq!(SAMPLE_COUNT.get(), 2);
    assert!(observation.resource.copied_bytes.is_none());
    assert!(
        observation.resource.held_bytes.is_none(),
        "incomplete accounting must stay unavailable"
    );
}

#[test]
fn snapshot_capture_samples_exclude_parsing_and_answer_encoding_but_wall_time_includes_them() {
    let mut fixture = Fixture::new();
    enable();
    let (answer, observation, encoded) = fixture.snapshot(2);
    assert!(matches!(answer, StateControlAnswer::Snapshot { .. }));
    assert!(
        !encoded.is_empty(),
        "answer buffer is retained at observation finish"
    );
    assert_eq!(
        recorded_events(),
        [
            "before",
            "serialize",
            "write",
            "after",
            "encode",
            "payload_encoded",
            "ended"
        ]
    );
    assert_capture_interval(&observation);
    assert_eq!(observation.resource.cpu_ns, Some(10.into()));
    assert_eq!(observation.disk.before.unwrap().read_bytes, 1.into());
    assert_eq!(observation.disk.after.unwrap().read_bytes, 2.into());
}

#[test]
fn snapshot_preflight_refusal_never_samples_resources() {
    for held in [false, true] {
        let mut fixture = Fixture::new();
        if held {
            fixture.snapshot(2);
        }
        enable();
        // The held case refuses in Session::control. The zero ID reaches the
        // local snapshot preflight with no retained capture, before sampling.
        let (answer, observation, _) =
            fixture.snapshot_with_capture_id(if held { 3 } else { 2 }, if held { 7 } else { 0 });
        assert!(matches!(answer, StateControlAnswer::ControlRefused { .. }));
        assert_eq!(SAMPLE_COUNT.get(), 0);
        assert_eq!(recorded_events(), ["encode", "payload_encoded", "ended"]);
        assert_eq!(
            observation.disk.unavailable_reason,
            Some(ObservationUnavailable::NotSampled)
        );
        assert!(
            observation
                .resource
                .unavailable
                .contains(&ResourceUnavailable {
                    field: ResourceField::CpuNs,
                    reason: ObservationUnavailable::NotSampled,
                })
        );
    }
}

#[test]
fn oversized_control_completion_keeps_fallback_encoding_inside_the_observation() {
    for initial_read in [false, true] {
        for encoding_fails in [false, true] {
            let common = fixtures::common(2);
            let mut pages = Pages::default();
            let mut page = pages.start(vec![b'x'; 32], 32.into(), 8).unwrap();
            if !initial_read {
                pages.publish_page();
                page = pages.next(page.snapshot_id, 6.into(), 8).unwrap();
            }
            let snapshot_id = page.snapshot_id;
            let offset = page.offset;
            let reserve = wrapper_reserve(&common, "control_answer").unwrap();
            page.octets = "A".repeat(if encoding_fails {
                STATE_ANSWER_LINE_BOUND + 1
            } else {
                STATE_ANSWER_LINE_BOUND - reserve + 1
            });
            let request = StateFrame::Control {
                common: common.clone(),
                frame: if initial_read {
                    StateControl::Read {
                        selector: ReadSelector::SystemPrompt {},
                    }
                } else {
                    StateControl::Next {
                        snapshot_id,
                        offset,
                    }
                },
            };
            let bytes = encode_state_line(&request, FrameDirection::Request).unwrap();
            enable();
            let (sample, _) = sampled_request(&bytes, 2).unwrap();
            let (observation, encoded) = complete(
                Reply::Control(StateControlAnswer::ReadPage(page)),
                sample,
                Some(CompletionBounds {
                    common: &common,
                    pages: &mut pages,
                    initial_read,
                }),
            )
            .unwrap();
            verify(&observation);
            assert_eq!(SAMPLE_COUNT.get(), 0);
            assert_eq!(
                EVENTS.with(|events| events
                    .borrow()
                    .iter()
                    .filter(|(event, _)| *event == "encode")
                    .count()),
                2,
                "oversized answer and its refusal both use the actual encoder"
            );
            let wire = wrap(&common, "control_answer", &encoded, &observation).unwrap();
            let answer = decode_state_line(&wire, FrameDirection::Response).unwrap();
            assert!(matches!(
                answer,
                StateFrame::ControlAnswer {
                    frame: StateControlAnswer::ControlRefused {
                        code: ControlRefusalCode::Read(ReadRefusalCode::AnswerTooLarge),
                        ..
                    },
                    ..
                }
            ));
            assert_eq!(pages.busy(), !initial_read);
            if !initial_read {
                assert_eq!(pages.held_bytes(), 32);
                assert_eq!(pages.next(snapshot_id, offset, 8).unwrap().offset, offset);
            }
            disable();
        }
    }
}

#[test]
fn snapshot_serialization_refusal_finishes_samples_before_answer_construction() {
    let mut fixture = Fixture::new();
    enable();
    IMAGE_FAILURE.set(true);
    let (answer, observation, _) = fixture.snapshot(2);
    assert!(matches!(answer, StateControlAnswer::ControlRefused { .. }));
    assert_eq!(
        recorded_events(),
        [
            "before",
            "serialize",
            "after",
            "encode",
            "payload_encoded",
            "ended"
        ]
    );
    assert_capture_interval(&observation);
    assert!(std::fs::read_dir(&fixture.root).unwrap().next().is_none());
}

#[test]
fn snapshot_write_refusal_samples_after_recoverable_cleanup_before_answer_encoding() {
    let mut fixture = Fixture::new();
    let collision = fixture.root.join("capture-7.sqlite.part");
    std::fs::write(&collision, b"stale owned file").unwrap();
    enable();
    let (answer, observation, _) = fixture.snapshot(2);
    assert!(matches!(answer, StateControlAnswer::ControlRefused { .. }));
    assert!(!collision.exists(), "recoverable cleanup finished");
    assert_eq!(
        recorded_events(),
        [
            "before",
            "serialize",
            "write",
            "cleanup_done",
            "after",
            "encode",
            "payload_encoded",
            "ended"
        ]
    );
    assert_capture_interval(&observation);
}

#[test]
fn unavailable_snapshot_counters_preserve_success_and_recoverable_refusal() {
    for refused in [false, true] {
        let mut fixture = Fixture::new();
        enable();
        UNAVAILABLE.set(true);
        IMAGE_FAILURE.set(refused);
        let (answer, observation, _) = fixture.snapshot(2);
        assert_eq!(
            matches!(answer, StateControlAnswer::ControlRefused { .. }),
            refused
        );
        assert_capture_interval(&observation);
        assert!(observation.resource.cpu_ns.is_none());
        assert_eq!(
            observation.disk.unavailable_reason,
            Some(ObservationUnavailable::Unsupported)
        );
        assert!(
            observation
                .resource
                .unavailable
                .contains(&ResourceUnavailable {
                    field: ResourceField::CpuNs,
                    reason: ObservationUnavailable::Denied,
                })
        );
    }
}
