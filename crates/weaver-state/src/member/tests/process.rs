//! STATE-OBSERVE's commit-before-answer boundary against the real member loop.
//! The child-only hook poisons metadata continuity after proving a content commit.
//! It creates a real record_observation refusal, not a SQLite engine failure.

use std::cell::Cell;
use std::fs::File;
use std::io::{BufRead, BufReader, Read, Write};
use std::os::fd::{AsRawFd, FromRawFd, OwnedFd};
use std::os::unix::net::UnixStream;
use std::os::unix::process::CommandExt;
use std::path::PathBuf;
use std::process::{Child, Command, Stdio};
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{Duration, Instant};

use super::super::*;
use weaver_state::{ServingStore, engine::serving_sqlite::seed_image};

#[allow(dead_code)]
#[path = "../../../tests/serving_member/fixtures.rs"]
pub(super) mod fixtures;

mod accounting;

const CHILD_TEST: &str = "member::tests::process::metadata_failure_child";
const CHILD_ROOM: &str = "WEAVER_TEST_METADATA_FAILURE_ROOM";
const CHILD_MODE: &str = "WEAVER_TEST_STATE_BOUNDARY_MODE";
static NEXT_ROOM: AtomicU64 = AtomicU64::new(0);

thread_local! {
    static INJECT_AFTER_COMMIT: Cell<bool> = const { Cell::new(false) };
    static COMMIT_VERIFIED: Cell<bool> = const { Cell::new(false) };
    static OBSERVATIONS_CHECKED: Cell<u64> = const { Cell::new(0) };
}

// Only the child test arms this hook. Ordinary unit tests and every production
// binary compile without any environment-controlled fault path.
pub(in crate::member) fn before_record_observation(
    store: &mut ServingSqlite,
    source: Option<&SourceBlockId>,
    observation: &StateOperationObservation,
) -> io::Result<()> {
    super::timing::verify(observation);
    OBSERVATIONS_CHECKED.with(|count| count.set(count.get() + 1));
    if !INJECT_AFTER_COMMIT.with(|armed| armed.replace(false)) {
        return Ok(());
    }
    assert_eq!(observation.operation, StateOperation::File);
    assert_eq!(observation.operation_id.0, 2);
    let source = source.expect("file must retain its exact source");
    assert_eq!(store.held().unwrap().0, 2, "seed plus committed content");
    let held = store
        .query(&ReadSelector::Statement {
            statement: StatementRef {
                block_id: source.clone(),
                binding: 0.into(),
            },
        })
        .unwrap();
    let QueryResult::Entries { entries } = held else {
        panic!("statement query must return content")
    };
    assert_eq!(entries.len(), 1);
    assert_eq!(entries[0].text, "Read committed.");
    let position = store.position().unwrap().expect("committed frontier");
    assert_eq!(&position.block_id, source);
    COMMIT_VERIFIED.with(|verified| verified.set(true));

    // Pre-occupy this ID through the actual metadata transaction. The unaltered
    // call in run must now fail its persisted contiguous-ID guard. Do not return
    // failure here: moving that real call after send must expose a bad receipt.
    store
        .record_observation(&StoredOperationObservation {
            request_source: Some(source.clone()),
            observation: observation.clone(),
        })
        .map_err(|error| invalid(error.to_string()))
}

#[test]
fn metadata_failure_child() {
    let Some(room_path) = std::env::var_os(CHILD_ROOM) else {
        return;
    };
    let room_path = PathBuf::from(room_path);
    let image = std::fs::read(room_path.join("image.sqlite")).unwrap();
    let room = Room::open(&room_path.join("room")).unwrap();
    // SAFETY: the parent installed one owned stream at fd3 for this exact test.
    let stream = unsafe { UnixStream::from_raw_fd(3) };
    let mut channel = Channel::new(stream).unwrap();
    let mode = std::env::var(CHILD_MODE).unwrap();
    assert!(matches!(
        mode.as_str(),
        "metadata" | "timing" | "oversized-control" | "accounting"
    ));
    if mode == "metadata" {
        INJECT_AFTER_COMMIT.with(|armed| armed.set(true));
    } else {
        super::timing::enable();
    }
    let result = run(&mut channel, &room, image);
    if mode == "metadata" {
        assert!(
            COMMIT_VERIFIED.with(Cell::get),
            "the content commit was not reached"
        );
        let error = result.expect_err("metadata continuity failure must end the member");
        assert!(
            error
                .to_string()
                .contains("operation ID is repeated, skipped or exhausted"),
            "wrong failure: {error}"
        );
    } else {
        result.unwrap();
        if mode == "accounting" {
            assert!(OBSERVATIONS_CHECKED.with(Cell::get) > 0);
        } else {
            assert_eq!(
                OBSERVATIONS_CHECKED.with(Cell::get),
                if mode == "oversized-control" { 4 } else { 2 }
            );
        }
    }
    drop(channel);
    room.clear_remnants().unwrap();
}

struct Fixture {
    child: Child,
    root: PathBuf,
}

impl Fixture {
    fn finish(&mut self) {
        let deadline = Instant::now() + Duration::from_secs(5);
        loop {
            if let Some(status) = self.child.try_wait().unwrap() {
                let log = std::fs::read_to_string(self.root.join("stderr")).unwrap();
                assert!(status.success(), "member child failed: {status}\n{log}");
                return;
            }
            assert!(
                Instant::now() < deadline,
                "member child failed to terminate"
            );
            std::thread::sleep(Duration::from_millis(5));
        }
    }
}

impl Drop for Fixture {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
        let _ = std::fs::remove_dir_all(&self.root);
    }
}

fn launch(mode: &str) -> (Fixture, BufReader<UnixStream>, SeedRequest) {
    let seed = fixtures::seed();
    let image = seed_image(&seed).unwrap().into_bytes();
    let root = std::env::temp_dir().join(format!(
        "weaver-metadata-boundary-{}-{}",
        std::process::id(),
        NEXT_ROOM.fetch_add(1, Ordering::Relaxed)
    ));
    std::fs::create_dir(&root).unwrap();
    std::fs::create_dir(root.join("room")).unwrap();
    std::fs::write(root.join("image.sqlite"), &image).unwrap();
    let (ours, theirs) = UnixStream::pair().unwrap();
    ours.set_read_timeout(Some(Duration::from_secs(5))).unwrap();
    ours.set_write_timeout(Some(Duration::from_secs(5)))
        .unwrap();
    // SAFETY: the fixture owns the live source. F_DUPFD_CLOEXEC returns a fresh
    // descriptor away from fd3, so exec installation cannot clobber the source.
    let raw = unsafe { nix::libc::fcntl(theirs.as_raw_fd(), nix::libc::F_DUPFD_CLOEXEC, 100) };
    assert!(raw >= 100);
    // SAFETY: F_DUPFD_CLOEXEC returned this fresh owned descriptor.
    let inherited = unsafe { OwnedFd::from_raw_fd(raw) };
    drop(theirs);
    let mut command = Command::new(std::env::current_exe().unwrap());
    command
        .args(["--exact", CHILD_TEST, "--nocapture"])
        .env(CHILD_ROOM, &root)
        .env(CHILD_MODE, mode)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(File::create(root.join("stderr")).unwrap());
    // SAFETY: only async-signal-safe descriptor duplication runs before exec.
    unsafe {
        command.pre_exec(move || {
            if nix::libc::dup2(raw, 3) < 0 {
                return Err(io::Error::last_os_error());
            }
            Ok(())
        });
    }
    let fixture = Fixture {
        child: command.spawn().unwrap(),
        root,
    };
    drop(inherited);
    let mut peer = BufReader::new(ours);
    let open = StateFrame::Open {
        common: fixtures::common(1),
        schema_id: seed.schema_id.clone(),
        scheme_id: seed.scheme_id.clone(),
        start: fixtures::start(&image, &seed),
        read_limits: fixtures::limits(1_000_000),
    };
    peer.get_mut()
        .write_all(&encode_state_line(&open, FrameDirection::Request).unwrap())
        .unwrap();
    let mut opened = Vec::new();
    assert!(peer.read_until(b'\n', &mut opened).unwrap() > 0);
    let opened_bytes = opened;
    let opened = decode_state_line(&opened_bytes, FrameDirection::Response).unwrap();
    assert!(matches!(opened, StateFrame::Opened { .. }));
    opened.validate_response_to(&open, 1.into()).unwrap();
    if mode == "accounting" {
        accounting::check_open(&open, &opened, &opened_bytes);
    }
    (fixture, peer, seed)
}

#[test]
fn metadata_persistence_failure_after_content_commit_closes_without_answer_bytes() {
    let (mut fixture, mut peer, seed) = launch("metadata");
    let file = StateFrame::File {
        common: fixtures::common(2),
        filing: fixtures::statement(&seed, 1, "Read committed."),
    };
    peer.get_mut()
        .write_all(&encode_state_line(&file, FrameDirection::Request).unwrap())
        .unwrap();
    let mut answer = [0; 1];
    assert_eq!(
        peer.read(&mut answer).unwrap(),
        0,
        "metadata failure must close before even one answer byte"
    );
    fixture.finish();
    assert!(
        std::fs::read_dir(fixture.root.join("room"))
            .unwrap()
            .next()
            .is_none()
    );
}

#[test]
fn file_and_pair_observations_start_before_the_real_operation_hint_parse() {
    let (mut fixture, mut peer, seed) = launch("timing");
    let statement = fixtures::statement(&seed, 1, "Read committed.");
    let opening = fixtures::opening(&statement);
    let pair = fixtures::pair(&seed, 2, PairRecord::Started(opening));
    for request in [
        StateFrame::File {
            common: fixtures::common(2),
            filing: statement,
        },
        StateFrame::FilePair {
            common: fixtures::common(3),
            filing: pair,
        },
    ] {
        peer.get_mut()
            .write_all(&encode_state_line(&request, FrameDirection::Request).unwrap())
            .unwrap();
        let mut bytes = Vec::new();
        assert!(
            peer.read_until(b'\n', &mut bytes).unwrap() > 0,
            "server failed the timing assertion before answering"
        );
        let answer = decode_state_line(&bytes, FrameDirection::Response).unwrap();
        answer
            .validate_response_to(&request, request.common().request_id)
            .unwrap();
        assert!(matches!(answer, StateFrame::Filed { .. }));
    }
    peer.get_ref().shutdown(std::net::Shutdown::Write).unwrap();
    let mut after = [0; 1];
    assert_eq!(peer.read(&mut after).unwrap(), 0);
    fixture.finish();
}

#[test]
fn serving_snapshot_and_preflight_refusal_finish_after_real_payload_encoding() {
    let (mut fixture, mut peer, _) = launch("timing");
    for request_id in [2, 3] {
        let request = StateFrame::Control {
            common: fixtures::common(request_id),
            frame: StateControl::Snapshot {
                capture_id: 7.into(),
            },
        };
        peer.get_mut()
            .write_all(&encode_state_line(&request, FrameDirection::Request).unwrap())
            .unwrap();
        let mut bytes = Vec::new();
        assert!(
            peer.read_until(b'\n', &mut bytes).unwrap() > 0,
            "serving run failed its encode/finish assertion before publishing"
        );
        let answer = decode_state_line(&bytes, FrameDirection::Response).unwrap();
        answer
            .validate_response_to(&request, request_id.into())
            .unwrap();
        let StateFrame::ControlAnswer {
            frame, observation, ..
        } = answer
        else {
            panic!("expected snapshot control answer");
        };
        if request_id == 2 {
            assert!(matches!(frame, StateControlAnswer::Snapshot { .. }));
            assert_eq!(observation.resource.cpu_ns, Some(10.into()));
        } else {
            assert!(matches!(
                frame,
                StateControlAnswer::ControlRefused {
                    code: ControlRefusalCode::Capture(CaptureRefusalCode::InvalidCaptureState),
                    ..
                }
            ));
            assert!(observation.resource.cpu_ns.is_none());
            assert_eq!(
                observation.disk.unavailable_reason,
                Some(ObservationUnavailable::NotSampled)
            );
        }
    }
    peer.get_ref().shutdown(std::net::Shutdown::Write).unwrap();
    let mut after = [0; 1];
    assert_eq!(peer.read(&mut after).unwrap(), 0);
    fixture.finish();
}

#[test]
fn serving_oversized_position_returns_answer_too_large_and_keeps_the_read_cursor() {
    fn exchange(peer: &mut BufReader<UnixStream>, request: &StateFrame) -> StateFrame {
        let bytes = encode_state_line(request, FrameDirection::Request).unwrap();
        assert!(bytes.len() <= STATE_REQUEST_LINE_BOUND);
        peer.get_mut().write_all(&bytes).unwrap();
        let mut answer = Vec::new();
        assert!(
            peer.read_until(b'\n', &mut answer).unwrap() > 0,
            "run closed instead of publishing its control-answer fallback"
        );
        let answer = decode_state_line(&answer, FrameDirection::Response).unwrap();
        answer
            .validate_response_to(request, request.common().request_id)
            .unwrap();
        answer
    }

    let (mut fixture, mut peer, seed) = launch("oversized-control");
    let mut filing = fixtures::statement(&seed, 1, "Read committed.");
    // Turn identities are opaque. This valid filing fits the 8 MiB request
    // allowance, and its Filed receipt carries only the small block identity.
    // Position must later encode the retained turn under the 1 MiB answer bound.
    filing.source.turn = Some("t".repeat(STATE_ANSWER_LINE_BOUND));
    assert!(matches!(
        exchange(
            &mut peer,
            &StateFrame::File {
                common: fixtures::common(2),
                filing,
            },
        ),
        StateFrame::Filed { .. }
    ));

    let answer = exchange(
        &mut peer,
        &StateFrame::Control {
            common: fixtures::common(3),
            frame: StateControl::Read {
                selector: ReadSelector::SystemPrompt {},
            },
        },
    );
    let StateFrame::ControlAnswer {
        frame: StateControlAnswer::ReadPage(page),
        ..
    } = answer
    else {
        panic!("the prompt read must acquire a cursor");
    };
    assert!(page.done);
    assert!(!page.decode_octets(1_000_000.into()).unwrap().is_empty());

    assert!(matches!(
        exchange(
            &mut peer,
            &StateFrame::Control {
                common: fixtures::common(4),
                frame: StateControl::Position {},
            },
        ),
        StateFrame::ControlAnswer {
            frame: StateControlAnswer::ControlRefused {
                code: ControlRefusalCode::Read(ReadRefusalCode::AnswerTooLarge),
                ..
            },
            ..
        }
    ));
    // The unrelated control refusal may roll back an unpublished page, but
    // must not discard the previously acquired cursor or terminate the session.
    assert!(matches!(
        exchange(
            &mut peer,
            &StateFrame::Control {
                common: fixtures::common(5),
                frame: StateControl::Release {
                    snapshot_id: page.snapshot_id,
                },
            },
        ),
        StateFrame::ControlAnswer {
            frame: StateControlAnswer::Released { snapshot_id },
            ..
        } if snapshot_id == page.snapshot_id
    ));
    peer.get_ref().shutdown(std::net::Shutdown::Write).unwrap();
    let mut after = [0; 1];
    assert_eq!(peer.read(&mut after).unwrap(), 0);
    fixture.finish();
}
