use super::*;
use std::sync::atomic::{AtomicU64, Ordering};

struct Fixture {
    root: std::path::PathBuf,
    room: Room,
}
impl Fixture {
    fn new() -> Self {
        static NEXT: AtomicU64 = AtomicU64::new(0);
        let root = std::env::temp_dir().join(format!(
            "state-capture-unit-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        std::fs::create_dir(&root).unwrap();
        let room = Room::open(&root).unwrap();
        Self { root, room }
    }
    fn held(&self) -> Capture {
        let name = self
            .room
            .write_part(7.into(), b"held raw image")
            .ok()
            .unwrap();
        Capture::Held {
            request: 3.into(),
            id: 7.into(),
            name,
        }
    }
}
impl Drop for Fixture {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.root);
    }
}
fn sample(op: StateOperation, request: u64) -> OperationSample {
    OperationSample::begin(1.into(), request.into(), op).unwrap()
}
fn hash() -> Digest {
    Digest::new("a".repeat(64)).unwrap()
}

#[test]
fn timeout_abort_deletes_both_held_and_finished_images_before_success() {
    for finish in [false, true] {
        let fixture = Fixture::new();
        let mut capture = fixture.held();
        if finish {
            let answer = capture
                .control(
                    &fixture.room,
                    3.into(),
                    &StateControl::Acknowledge {
                        capture_id: 7.into(),
                        dump_sha256: hash(),
                    },
                    &mut sample(StateOperation::Acknowledge, 3),
                )
                .unwrap();
            assert_eq!(
                answer,
                StateControlAnswer::Finished {
                    capture_id: 7.into(),
                    room_name: "capture-7.sqlite".into()
                }
            );
            assert!(!fixture.root.join("capture-7.sqlite.part").exists());
            assert!(fixture.root.join("capture-7.sqlite").exists());
        }
        assert_eq!(
            capture
                .control(
                    &fixture.room,
                    3.into(),
                    &StateControl::Abort {
                        capture_id: 7.into()
                    },
                    &mut sample(StateOperation::Abort, 3)
                )
                .unwrap(),
            StateControlAnswer::Aborted {
                capture_id: 7.into()
            }
        );
        assert!(!fixture.root.join("capture-7.sqlite.part").exists());
        assert!(!fixture.root.join("capture-7.sqlite").exists());
        assert!(matches!(capture, Capture::None));
    }
}

#[test]
fn failed_deletion_never_answers_aborted_or_releases_capture() {
    let fixture = Fixture::new();
    let mut capture = fixture.held();
    std::fs::remove_file(fixture.root.join("capture-7.sqlite.part")).unwrap();
    assert!(
        capture
            .control(
                &fixture.room,
                3.into(),
                &StateControl::Abort {
                    capture_id: 7.into()
                },
                &mut sample(StateOperation::Abort, 3)
            )
            .is_err()
    );
    assert!(capture.held());
    assert_eq!(capture.continuation(), Some(3.into()));
}

#[test]
fn capture_refusals_preserve_identity_and_finished_release_needs_fresh_exchange() {
    let fixture = Fixture::new();
    let mut capture = fixture.held();
    for control in [
        StateControl::Abort {
            capture_id: 8.into(),
        },
        StateControl::Read {
            selector: ReadSelector::SystemPrompt {},
        },
        StateControl::ReleaseCapture {
            capture_id: 7.into(),
        },
    ] {
        let answer = capture
            .control(
                &fixture.room,
                3.into(),
                &control,
                &mut sample(control.operation(), 3),
            )
            .unwrap();
        assert!(matches!(
            answer,
            StateControlAnswer::ControlRefused {
                code: ControlRefusalCode::Capture(CaptureRefusalCode::InvalidCaptureState),
                ..
            }
        ));
        assert!(capture.held());
        assert!(fixture.root.join("capture-7.sqlite.part").exists());
    }
    capture
        .control(
            &fixture.room,
            3.into(),
            &StateControl::Acknowledge {
                capture_id: 7.into(),
                dump_sha256: hash(),
            },
            &mut sample(StateOperation::Acknowledge, 3),
        )
        .unwrap();
    assert!(matches!(
        capture
            .control(
                &fixture.room,
                3.into(),
                &StateControl::ReleaseCapture {
                    capture_id: 7.into()
                },
                &mut sample(StateOperation::ReleaseCapture, 3)
            )
            .unwrap(),
        StateControlAnswer::ControlRefused { .. }
    ));
    assert_eq!(
        capture
            .control(
                &fixture.room,
                4.into(),
                &StateControl::ReleaseCapture {
                    capture_id: 7.into()
                },
                &mut sample(StateOperation::ReleaseCapture, 4)
            )
            .unwrap(),
        StateControlAnswer::CaptureReleased {
            capture_id: 7.into()
        }
    );
    assert!(matches!(capture, Capture::None));
    assert!(!fixture.root.join("capture-7.sqlite").exists());
}

#[test]
fn refused_capture_accepts_only_correlated_cleanup_before_fresh_request() {
    let fixture = Fixture::new();
    let mut capture = Capture::Refused {
        request: 9.into(),
        id: 2.into(),
    };
    assert!(capture.allows_continuation(&StateControl::Abort {
        capture_id: 2.into()
    }));
    assert!(!capture.allows_continuation(&StateControl::Acknowledge {
        capture_id: 2.into(),
        dump_sha256: hash()
    }));
    assert!(matches!(
        capture
            .control(
                &fixture.room,
                9.into(),
                &StateControl::Abort {
                    capture_id: 3.into()
                },
                &mut sample(StateOperation::Abort, 9)
            )
            .unwrap(),
        StateControlAnswer::ControlRefused { .. }
    ));
    assert_eq!(capture.continuation(), Some(9.into()));
    assert_eq!(
        capture
            .control(
                &fixture.room,
                9.into(),
                &StateControl::Abort {
                    capture_id: 2.into()
                },
                &mut sample(StateOperation::Abort, 9)
            )
            .unwrap(),
        StateControlAnswer::Aborted {
            capture_id: 2.into()
        }
    );
    assert_eq!(capture.continuation(), None);
    capture = Capture::Refused {
        request: 9.into(),
        id: 2.into(),
    };
    capture.fresh();
    assert_eq!(capture.continuation(), None);
}
