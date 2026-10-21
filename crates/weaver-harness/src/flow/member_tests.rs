//! Cross-process fixture. The harness links no store crate. Build the matching
//! normal member and pass WEAVER_STATE_BINARY when running this ignored test.
//! The seed certificate here proves synthetic byte mechanics only.

use std::fs::File;
use std::io::{BufRead, BufReader, Write};
use std::os::fd::{AsRawFd, FromRawFd, OwnedFd};
use std::os::unix::net::UnixStream;
use std::os::unix::process::CommandExt;
use std::path::Path;
use std::process::{Child, Command, Stdio};
use std::time::{Duration, Instant};

use super::RecordedStateMeasurement;
use super::channel::{StateChannel, StateCompletion, StateRequest};
use weaver_types::state_flow::*;

struct Process(Child);
impl Drop for Process {
    fn drop(&mut self) {
        let _ = self.0.kill();
        let _ = self.0.wait();
    }
}

fn high_copy(fd: i32) -> OwnedFd {
    // SAFETY: fcntl duplicates a borrowed live fd into a fresh owned number.
    let copied = unsafe { nix::libc::fcntl(fd, nix::libc::F_DUPFD_CLOEXEC, 100) };
    assert!(copied >= 100);
    // SAFETY: the descriptor just returned by fcntl has no other owner.
    unsafe { OwnedFd::from_raw_fd(copied) }
}

fn launch(binary: &Path, room: &Path, image: Option<&Path>) -> (Process, UnixStream) {
    let (ours, theirs) = UnixStream::pair().unwrap();
    ours.set_read_timeout(Some(Duration::from_secs(10)))
        .unwrap();
    ours.set_write_timeout(Some(Duration::from_secs(10)))
        .unwrap();
    let door = high_copy(theirs.as_raw_fd());
    let lock_file = File::create(room.join("../run.lock")).unwrap();
    // SAFETY: flock changes only the fixture-owned run lock.
    assert_eq!(
        unsafe { nix::libc::flock(lock_file.as_raw_fd(), nix::libc::LOCK_EX) },
        0
    );
    let lock = high_copy(lock_file.as_raw_fd());
    let image = image.map(|path| high_copy(File::open(path).unwrap().as_raw_fd()));
    let door_fd = door.as_raw_fd();
    let lock_fd = lock.as_raw_fd();
    let image_fd = image.as_ref().map(AsRawFd::as_raw_fd);
    let mut command = Command::new(binary);
    command
        .arg(if image.is_some() {
            "--mode=serving"
        } else {
            "--mode=seed"
        })
        .arg(room)
        .stdin(Stdio::null())
        .stdout(Stdio::null());
    // SAFETY: async-signal-safe descriptor operations only, with source fds
    // above every destination. No destination can clobber an unread source.
    unsafe {
        command.pre_exec(move || {
            if nix::libc::dup2(door_fd, 3) < 0 || nix::libc::dup2(lock_fd, 9) < 0 {
                return Err(std::io::Error::last_os_error());
            }
            if let Some(image) = image_fd {
                if nix::libc::dup2(image, 4) < 0 {
                    return Err(std::io::Error::last_os_error());
                }
            } else {
                nix::libc::close(4);
            }
            Ok(())
        });
    }
    (Process(command.spawn().unwrap()), ours)
}

fn now() -> std::io::Result<u64> {
    let mut time = nix::libc::timespec {
        tv_sec: 0,
        tv_nsec: 0,
    };
    // SAFETY: clock_gettime initializes a valid stack timespec.
    if unsafe { nix::libc::clock_gettime(nix::libc::CLOCK_MONOTONIC, &mut time) } != 0 {
        return Err(std::io::Error::last_os_error());
    }
    Ok(u64::try_from(time.tv_sec)
        .unwrap()
        .checked_mul(1_000_000_000)
        .unwrap()
        .checked_add(u64::try_from(time.tv_nsec).unwrap())
        .unwrap())
}

fn exchange(channel: &mut StateChannel, request: StateRequest) -> StateCompletion {
    channel.begin(request, now().unwrap()).unwrap();
    let deadline = Instant::now() + Duration::from_secs(10);
    while channel.held_measurement().is_none() {
        assert!(
            Instant::now() < deadline,
            "member answer timed out in fixture"
        );
        channel.progress(&mut now).unwrap();
        if channel.held_measurement().is_none() {
            std::thread::sleep(Duration::from_millis(1));
        }
    }
    let measure = channel.held_measurement().unwrap().clone();
    measure.validate().unwrap();
    // Actual record-barrier integration is exercised independently. This fixture
    // isolates the normal member process from trace capacity scheduling.
    channel
        .release(RecordedStateMeasurement::for_test_in("run", &measure))
        .unwrap()
        .expect("recorded completion")
}

fn statement(seed: &SeedRequest) -> StatementFile {
    StatementFile {
        block_id: SourceBlockId {
            run: "run".into(),
            sequence: 1.into(),
            block: 0.into(),
        },
        source: SourceEnvelope {
            session: "session".into(),
            run: "run".into(),
            turn: Some("turn".into()),
            sequence: 1.into(),
            kind: "message.assistant".into(),
            channel: "output".into(),
            origin: "model".into(),
            subsystem: "harness".into(),
            causal_parent: None,
            wall_ms: 0,
            monotonic_ns: 1.into(),
        },
        partition_rule_id: "statements".into(),
        source_path: vec![
            SourcePathPart::Key("content".into()),
            SourcePathPart::Index { index: 0.into() },
        ],
        admission: Admission::Admitted,
        raw_block: seed.raw_block.clone(),
        text: seed.prompt_text.clone(),
        scheme_id: seed.scheme_id.clone(),
        bindings: seed.bindings.clone(),
        context: None,
        effect: None,
    }
}

#[test]
#[ignore = "requires WEAVER_STATE_BINARY built from the matching revision"]
fn normal_member_seed_open_file_read_and_capture_cleanup() {
    let binary = std::env::var_os("WEAVER_STATE_BINARY").expect("set matching member binary");
    let scratch = crate::scratch::dir(format!("weaver-flow-member-{}", std::process::id()));
    let seed_room = scratch.join("seed-room");
    let room = scratch.join("room");
    std::fs::create_dir(&seed_room).unwrap();
    std::fs::create_dir(&room).unwrap();
    let seed: SeedRequest = decode_strict(include_str!("fixtures/seed.json")).unwrap();
    let (seeder, socket) = launch(Path::new(&binary), &seed_room, None);
    let mut socket = BufReader::new(socket);
    socket
        .get_mut()
        .write_all(
            &encode_seed_line(
                &SeedFrame::Seed {
                    request: Box::new(seed.clone()),
                },
                FrameDirection::Request,
            )
            .unwrap(),
        )
        .unwrap();
    let mut line = Vec::new();
    socket.read_until(b'\n', &mut line).unwrap();
    let SeedFrame::Created { created } = decode_seed_line(&line, FrameDirection::Response).unwrap()
    else {
        panic!("seed refused")
    };
    let image = scratch.join("image.sqlite");
    std::fs::copy(seed_room.join(created.room_name), &image).unwrap();
    drop(socket);
    drop(seeder);
    // Hashing is test setup, not a dependency or a runtime harness operation.
    let hash = Command::new("sha256sum").arg(&image).output().unwrap();
    assert!(hash.status.success());
    let digest = String::from_utf8(hash.stdout)
        .unwrap()
        .split_whitespace()
        .next()
        .unwrap()
        .to_owned();
    let (_member, stream) = launch(Path::new(&binary), &room, Some(&image));
    let mut channel = StateChannel::new(stream, "session".into(), "run".into()).unwrap();
    let answer = exchange(
        &mut channel,
        StateRequest::Open {
            schema_id: STATE_CONTENT_SCHEMA.into(),
            scheme_id: seed.scheme_id.clone(),
            start: StartIdentity {
                seed_id: seed.seed_id.clone(),
                artifact_id: "synthetic-seed".into(),
                dump_sha256: Digest::new(digest).unwrap(),
                config_sha256: Digest::new("b".repeat(64)).unwrap(),
                parent_artifact_id: None,
                position: None,
            },
            read_limits: ReadLimits {
                max_result_bytes: 1_000_000.into(),
                measurement_ref: "test-only".into(),
            },
        },
    );
    assert!(matches!(answer.answer, StateFrame::Opened { .. }));
    let filed = exchange(&mut channel, StateRequest::File(statement(&seed)));
    assert!(matches!(filed.answer, StateFrame::Filed { .. }));
    let read = exchange(
        &mut channel,
        StateRequest::Control(StateControl::Read {
            selector: ReadSelector::Subjects {
                nouns: vec!["item".into()],
            },
        }),
    );
    let StateFrame::ControlAnswer {
        frame: StateControlAnswer::ReadPage(page),
        ..
    } = read.answer
    else {
        panic!("subject query did not return a page");
    };
    assert!(page.done, "the tiny fixture fits one page");
    let bytes = page.decode_octets(1_000_000.into()).unwrap();
    let result: QueryResult = decode_strict(std::str::from_utf8(&bytes).unwrap()).unwrap();
    let QueryResult::Entries { entries } = result else {
        panic!("wrong query result")
    };
    assert!(entries.iter().any(|entry| entry.text == seed.prompt_text
        && entry.raw_block.get() == seed.raw_block.get()
        && matches!(&entry.source, ReadSource::Event(source)
            if source.block_id == statement(&seed).block_id)));
    let released = exchange(
        &mut channel,
        StateRequest::Control(StateControl::Release {
            snapshot_id: page.snapshot_id,
        }),
    );
    assert!(matches!(
        released.answer,
        StateFrame::ControlAnswer {
            frame: StateControlAnswer::Released { .. },
            ..
        }
    ));
    let shape = exchange(&mut channel, StateRequest::Control(StateControl::Shape {}));
    assert!(matches!(
        shape.answer,
        StateFrame::ControlAnswer {
            frame: StateControlAnswer::Shape(_),
            ..
        }
    ));
    // Correlated abort of a held capture must return the member to Ready.
    let snapshot = exchange(
        &mut channel,
        StateRequest::Control(StateControl::Snapshot {
            capture_id: 1.into(),
        }),
    );
    let StateFrame::ControlAnswer {
        frame: StateControlAnswer::Snapshot { .. },
        ..
    } = snapshot.answer
    else {
        panic!("snapshot failed")
    };
    // Abort the held image and verify a subsequent ordinary read succeeds.
    let aborted = exchange(
        &mut channel,
        StateRequest::Control(StateControl::Abort {
            capture_id: 1.into(),
        }),
    );
    assert!(matches!(
        aborted.answer,
        StateFrame::ControlAnswer {
            frame: StateControlAnswer::Aborted { .. },
            ..
        }
    ));
    let held = exchange(&mut channel, StateRequest::Control(StateControl::Held {}));
    assert!(matches!(
        held.answer,
        StateFrame::ControlAnswer {
            frame: StateControlAnswer::Held { .. },
            ..
        }
    ));
    assert_eq!(
        std::fs::read_dir(&room).unwrap().count(),
        0,
        "abort removes room image"
    );
}

#[test]
#[ignore = "requires WEAVER_STATE_BINARY built from the matching revision"]
fn normal_member_paged_read_reassembles_exact_content_and_releases_snapshot() {
    use super::read::{PageRead, ReadAdvance};

    let binary = std::env::var_os("WEAVER_STATE_BINARY").expect("set matching member binary");
    let scratch = crate::scratch::dir(format!("weaver-paged-member-{}", std::process::id()));
    let seed_room = scratch.join("seed-room");
    let room = scratch.join("room");
    std::fs::create_dir(&seed_room).unwrap();
    std::fs::create_dir(&room).unwrap();
    let seed: SeedRequest = decode_strict(include_str!("fixtures/seed.json")).unwrap();
    let (seeder, socket) = launch(Path::new(&binary), &seed_room, None);
    let mut socket = BufReader::new(socket);
    socket
        .get_mut()
        .write_all(
            &encode_seed_line(
                &SeedFrame::Seed {
                    request: Box::new(seed.clone()),
                },
                FrameDirection::Request,
            )
            .unwrap(),
        )
        .unwrap();
    let mut line = Vec::new();
    socket.read_until(b'\n', &mut line).unwrap();
    let SeedFrame::Created { created } = decode_seed_line(&line, FrameDirection::Response).unwrap()
    else {
        panic!("seed refused")
    };
    let image = scratch.join("image.sqlite");
    std::fs::copy(seed_room.join(created.room_name), &image).unwrap();
    drop(socket);
    drop(seeder);
    let hash = Command::new("sha256sum").arg(&image).output().unwrap();
    assert!(hash.status.success());
    let digest = String::from_utf8(hash.stdout)
        .unwrap()
        .split_whitespace()
        .next()
        .unwrap()
        .to_owned();
    let (_member, stream) = launch(Path::new(&binary), &room, Some(&image));
    let mut channel = StateChannel::new(stream, "session".into(), "run".into()).unwrap();
    let opened = exchange(
        &mut channel,
        StateRequest::Open {
            schema_id: STATE_CONTENT_SCHEMA.into(),
            scheme_id: seed.scheme_id.clone(),
            start: StartIdentity {
                seed_id: seed.seed_id.clone(),
                artifact_id: "synthetic-seed".into(),
                dump_sha256: Digest::new(digest).unwrap(),
                config_sha256: Digest::new("b".repeat(64)).unwrap(),
                parent_artifact_id: None,
                position: None,
            },
            read_limits: ReadLimits {
                max_result_bytes: 4_000_000.into(),
                measurement_ref: "test-only-large-read".into(),
            },
        },
    );
    assert!(matches!(opened.answer, StateFrame::Opened { .. }));
    let mut filed = statement(&seed);
    filed.text.push_str(&" ".repeat(600_000));
    filed.raw_block = serde_json::value::to_raw_value(&serde_json::json!({
        "type":"text", "text":filed.text,
    }))
    .unwrap();
    let answer = exchange(&mut channel, StateRequest::File(filed.clone()));
    assert!(matches!(answer.answer, StateFrame::Filed { .. }));
    let selector = ReadSelector::Subjects {
        nouns: vec!["item".into()],
    };
    // The actual recorder barrier has its separate context tests. This fixture
    // uses exchange's explicit proof double only for measurement release, and
    // tests actual SQL, immutable paging and the read owner's protocol together.
    let mut first_snapshot = None;
    for attempt in 0..2 {
        let mut read = PageRead::new(selector.clone(), 4_000_000.into()).unwrap();
        let mut pages = 0;
        let mut releases = 0;
        let mut raw_pages = Vec::new();
        let mut finished = false;
        for _ in 0..32 {
            let request = read.next_control().expect("read owes a request");
            let completion = exchange(&mut channel, StateRequest::Control(request.clone()));
            // The channel assigned and completed this exact request before the
            // core sees its ID. Binding still precedes accepting any result.
            read.bind_request(completion.answer.common().request_id, &request)
                .unwrap();
            let StateFrame::ControlAnswer { common, frame, .. } = &completion.answer else {
                panic!("read did not receive a control answer")
            };
            match frame {
                StateControlAnswer::ReadPage(page) => {
                    pages += 1;
                    raw_pages.extend(page.decode_octets(4_000_000.into()).unwrap());
                }
                StateControlAnswer::Released { .. } => releases += 1,
                other => panic!("normal member refused the read: {other:?}"),
            }
            let progress = read.accept(common.request_id, &request, frame).unwrap();
            if progress == ReadAdvance::Complete {
                finished = true;
                break;
            }
            assert!(read.take().is_none(), "a partial result escaped");
        }
        assert!(finished, "bounded fixture did not complete");
        assert!(pages >= 3, "fixture did not cross actual page boundaries");
        assert_eq!(releases, 1);
        let completed = read.take().unwrap().unwrap();
        assert_eq!(completed.selector(), &selector);
        assert_eq!(completed.raw_bytes(), raw_pages);
        let QueryResult::Entries { entries } = completed.result() else {
            panic!("entries")
        };
        let [entry] = entries.as_slice() else {
            panic!("one filed source")
        };
        assert_eq!(entry.text, filed.text);
        assert_eq!(entry.raw_block.get(), filed.raw_block.get());
        assert_eq!(entry.bindings[0].ordinal, 0.into());
        assert_eq!(entry.bindings[0].value, filed.bindings[0]);
        assert!(
            matches!(&entry.source, ReadSource::Event(source) if source.block_id == filed.block_id)
        );
        assert!(read.next_control().is_none());
        assert!(read.take().is_none());
        if attempt == 0 {
            first_snapshot = Some(completed.snapshot_id());
        } else {
            assert!(completed.snapshot_id() > first_snapshot.unwrap());
        }
    }
}
