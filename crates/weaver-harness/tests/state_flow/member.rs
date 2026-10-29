//! A normal SQLite member behind a byte-preserving test proxy. The seed and
//! language answers are synthetic. No store crate or hidden query is linked.
use std::fs::File;
use std::io::{self, BufRead, BufReader, Read, Write};
use std::os::fd::{AsRawFd, FromRawFd, OwnedFd};
use std::os::unix::net::UnixStream;
use std::os::unix::process::{CommandExt, ExitStatusExt};
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::time::{Duration, Instant};
use weaver_types::state_flow as s;

pub(crate) fn digest(bytes: &[u8]) -> s::Digest {
    use sha2::{Digest as _, Sha256};
    s::Digest::new(format!("{:x}", Sha256::digest(bytes))).unwrap()
}

fn binary() -> PathBuf {
    let path = std::env::current_exe()
        .unwrap()
        .parent()
        .unwrap()
        .parent()
        .unwrap()
        .join("weaver-state");
    assert!(
        path.is_file(),
        "build the normal member in this target/profile: cargo build -p weaver-state --locked --features sqlite --bin weaver-state"
    );
    path
}

fn high(fd: i32) -> OwnedFd {
    // SAFETY: fd is borrowed from a live owner; the result is a fresh descriptor.
    let copy = unsafe { nix::libc::fcntl(fd, nix::libc::F_DUPFD_CLOEXEC, 100) };
    assert!(copy >= 100);
    // SAFETY: this newly allocated descriptor has no other owner.
    unsafe { OwnedFd::from_raw_fd(copy) }
}

struct Process {
    child: Child,
    root: PathBuf,
    room: PathBuf,
    killed: bool,
}
impl Process {
    fn launch(root: &Path, image: Option<&[u8]>) -> (Self, UnixStream) {
        Self::launch_in(root, image, None)
    }

    fn launch_in(
        root: &Path,
        image: Option<&[u8]>,
        shared: Option<(&File, &Path)>,
    ) -> (Self, UnixStream) {
        std::fs::create_dir(root).unwrap();
        let room = if let Some((_, room)) = shared {
            room.to_path_buf()
        } else {
            let room = root.join("room");
            std::fs::create_dir(&room).unwrap();
            room
        };
        let (ours, theirs) = UnixStream::pair().unwrap();
        ours.set_read_timeout(Some(Duration::from_secs(5))).unwrap();
        ours.set_write_timeout(Some(Duration::from_secs(5)))
            .unwrap();
        let door = high(theirs.as_raw_fd());
        let lock = if let Some((lock, _)) = shared {
            high(lock.as_raw_fd())
        } else {
            let lock = File::create(root.join("run.lock")).unwrap();
            // SAFETY: this fixture owns the private lock file across fork/exec.
            assert_eq!(
                unsafe { nix::libc::flock(lock.as_raw_fd(), nix::libc::LOCK_EX) },
                0
            );
            high(lock.as_raw_fd())
        };
        let image = image.map(|bytes| {
            let path = root.join("input.sqlite");
            std::fs::write(&path, bytes).unwrap();
            high(File::open(path).unwrap().as_raw_fd())
        });
        let door_fd = door.as_raw_fd();
        let lock_fd = lock.as_raw_fd();
        let image_fd = image.as_ref().map(AsRawFd::as_raw_fd);
        let mut command = Command::new(binary());
        command
            .arg(if image.is_some() {
                "--mode=serving"
            } else {
                "--mode=seed"
            })
            .arg(&room)
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(File::create(root.join("stderr")).unwrap());
        // SAFETY: only async-signal-safe descriptor calls run after fork. High
        // sources cannot be overwritten by fd3/fd4/fd9 installation.
        unsafe {
            command.pre_exec(move || {
                if nix::libc::dup2(door_fd, 3) < 0 || nix::libc::dup2(lock_fd, 9) < 0 {
                    return Err(io::Error::last_os_error());
                }
                if let Some(fd) = image_fd {
                    if nix::libc::dup2(fd, 4) < 0 {
                        return Err(io::Error::last_os_error());
                    }
                } else {
                    nix::libc::close(4);
                }
                Ok(())
            });
        }
        let child = command.spawn().unwrap();
        (
            Self {
                child,
                root: root.into(),
                room,
                killed: false,
            },
            ours,
        )
    }

    fn exited(&mut self) -> bool {
        let Some(status) = self.child.try_wait().unwrap() else {
            return false;
        };
        assert!(
            if self.killed {
                status.signal() == Some(nix::libc::SIGKILL)
            } else {
                status.success()
            },
            "normal member failed: {}",
            std::fs::read_to_string(self.root.join("stderr")).unwrap()
        );
        assert_eq!(
            std::fs::read_dir(&self.room).unwrap().count(),
            0,
            "normal member retained unpublished room files"
        );
        true
    }

    fn wait(&mut self) {
        let end = Instant::now() + Duration::from_secs(5);
        while !self.exited() {
            assert!(Instant::now() < end, "normal member did not exit after EOF");
            std::thread::sleep(Duration::from_millis(1));
        }
    }

    fn kill_and_wait(&mut self) {
        assert!(!self.killed);
        self.child.kill().unwrap();
        self.killed = true;
        self.wait();
        std::fs::write(self.root.join("killed-and-reaped"), b"SIGKILL\n").unwrap();
    }
}
impl Drop for Process {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum ReceiptTreatment {
    Hold,
    Fragment,
    WrongIdentity,
    Discard,
    Kill,
}

#[derive(Clone, Copy)]
pub(super) enum CaptureDelay {
    Snapshot,
    Finished,
}

pub(super) struct NormalMember {
    process: Process,
    stream: UnixStream,
    to_member: Vec<u8>,
    to_harness: Vec<u8>,
    request_bytes: Vec<u8>,
    response_bytes: Vec<u8>,
    response_lines: Vec<u8>,
    treatment: Option<ReceiptTreatment>,
    intercepted: bool,
    fault_applied: bool,
    held_prefix: usize,
    pub(super) corrupt_release: bool,
    pub(super) corrupt_abort: bool,
    pub(super) refuse_first_snapshot: bool,
    pub(super) damage_first_image: bool,
    pub(super) wait_for_capture_join: bool,
    held_filing: Option<Vec<u8>>,
    released_filing: Option<Vec<u8>>,
    capture_delay: Option<CaptureDelay>,
    held_capture_result: Option<Vec<u8>>,
    request_lines: Vec<u8>,
    harness_eof: bool,
    member_eof: bool,
    write_closed: bool,
    output_closed: bool,
    pub(super) image: Vec<u8>,
}

impl NormalMember {
    pub(super) fn seeded(root: &Path, scheme: &str, hold_filing: bool) -> Self {
        let image = Self::seed_image(root, scheme);
        Self::from_image(root, image, hold_filing, None)
    }

    pub(super) fn seed_image(root: &Path, scheme: &str) -> Vec<u8> {
        let mut seed: s::SeedRequest =
            s::decode_strict(include_str!("../../src/flow/fixtures/seed.json")).unwrap();
        seed.ste100_scheme = scheme.into();
        seed.scheme_id = format!("ste100:{}", digest(scheme.as_bytes()));
        seed.certificate.scheme_id = seed.scheme_id.clone();
        let (mut seeder, stream) = Process::launch(&root.join("normal-seed"), None);
        let mut channel = BufReader::new(stream);
        channel
            .get_mut()
            .write_all(
                &s::encode_seed_line(
                    &s::SeedFrame::Seed {
                        request: Box::new(seed.clone()),
                    },
                    s::FrameDirection::Request,
                )
                .unwrap(),
            )
            .unwrap();
        let mut response = Vec::new();
        channel.read_until(b'\n', &mut response).unwrap();
        let answer = s::decode_seed_line(&response, s::FrameDirection::Response).unwrap();
        answer.validate_response_to(&seed).unwrap();
        let s::SeedFrame::Created { created } = answer else {
            panic!("normal seed refused: {answer:?}");
        };
        let image = std::fs::read(seeder.root.join("room").join(created.room_name)).unwrap();
        assert_eq!(image.len() as u64, created.size_bytes.0);
        assert_eq!(&image[..16], b"SQLite format 3\0");
        drop(channel);
        seeder.wait();
        image
    }

    pub(super) fn from_image(
        root: &Path,
        image: Vec<u8>,
        hold_filing: bool,
        shared: Option<(&File, &Path)>,
    ) -> Self {
        let (process, stream) =
            Process::launch_in(&root.join("normal-member"), Some(&image), shared);
        stream.set_nonblocking(true).unwrap();
        Self {
            process,
            stream,
            to_member: vec![],
            to_harness: vec![],
            request_bytes: vec![],
            response_bytes: vec![],
            response_lines: vec![],
            treatment: hold_filing.then_some(ReceiptTreatment::Hold),
            intercepted: false,
            fault_applied: false,
            held_prefix: 0,
            corrupt_release: false,
            corrupt_abort: false,
            refuse_first_snapshot: false,
            damage_first_image: false,
            wait_for_capture_join: false,
            held_filing: None,
            released_filing: None,
            capture_delay: None,
            held_capture_result: None,
            request_lines: vec![],
            harness_eof: false,
            member_eof: false,
            write_closed: false,
            output_closed: false,
            image,
        }
    }

    pub(super) fn treat_first_filing(&mut self, treatment: ReceiptTreatment) {
        assert!(!self.intercepted);
        self.treatment = Some(treatment);
    }

    pub(super) fn delay_first_capture_result(&mut self, delay: CaptureDelay) {
        assert!(self.capture_delay.is_none() && self.held_capture_result.is_none());
        self.capture_delay = Some(delay);
    }

    pub(super) fn release_held_filing(&mut self) {
        assert!(matches!(
            self.treatment,
            Some(ReceiptTreatment::Hold | ReceiptTreatment::Fragment)
        ));
        let bytes = self.held_filing.take().expect("original held receipt");
        self.to_harness
            .extend_from_slice(&bytes[self.held_prefix..]);
        self.released_filing = Some(bytes);
    }

    /// Apply a failure only after the circuit has observed pending custody.
    /// The complete, unmodified receipt stays held as evidence in every case.
    pub(super) fn apply_filing_fault(&mut self) {
        assert!(!self.fault_applied);
        let original = self.held_filing.as_ref().unwrap();
        match self.treatment.unwrap() {
            ReceiptTreatment::WrongIdentity => {
                let mut response =
                    s::decode_state_line(original, s::FrameDirection::Response).unwrap();
                let s::StateFrame::Filed { common, .. } = &mut response else {
                    panic!("intercepted committed receipt");
                };
                common.request_id.0 += 1;
                let damaged = s::encode_state_line(&response, s::FrameDirection::Response).unwrap();
                std::fs::write(
                    self.process
                        .root
                        .parent()
                        .unwrap()
                        .join("damaged-filing.jsonl"),
                    &damaged,
                )
                .unwrap();
                self.to_harness.extend(damaged);
            }
            ReceiptTreatment::Discard => (),
            ReceiptTreatment::Kill => self.process.kill_and_wait(),
            other => panic!("not a filing failure: {other:?}"),
        }
        self.fault_applied = true;
    }

    pub(super) fn step(
        &mut self,
        peer: &mut UnixStream,
        requests: &mut Vec<String>,
        filings: &mut Vec<s::StateFrame>,
    ) {
        if self.held_filing.is_some()
            && matches!(
                self.treatment,
                Some(ReceiptTreatment::Hold | ReceiptTreatment::Fragment)
            )
            && self
                .process
                .root
                .parent()
                .unwrap()
                .join("ready-rollback-lower.json")
                .exists()
        {
            let bytes = self.held_filing.take().unwrap();
            self.to_harness
                .extend_from_slice(&bytes[self.held_prefix..]);
            self.released_filing = Some(bytes);
        }
        if !self.harness_eof {
            let mut bytes = [0; 65536];
            match peer.read(&mut bytes) {
                Ok(0) => self.harness_eof = true,
                Ok(n) => {
                    assert!(
                        self.held_filing.is_none(),
                        "worker sent another member request before the committed filing receipt was released"
                    );
                    self.request_bytes.extend_from_slice(&bytes[..n]);
                    self.request_lines.extend_from_slice(&bytes[..n]);
                    self.to_member.extend_from_slice(&bytes[..n]);
                }
                Err(e) if e.kind() == io::ErrorKind::WouldBlock => (),
                Err(e) => panic!("normal member request proxy: {e}"),
            }
        }
        while let Some(end) = self.request_lines.iter().position(|b| *b == b'\n') {
            let line: Vec<_> = self.request_lines.drain(..=end).collect();
            let frame = s::decode_state_line(&line, s::FrameDirection::Request).unwrap();
            if self.refuse_first_snapshot
                && let s::StateFrame::Control {
                    frame: s::StateControl::Snapshot { capture_id },
                    ..
                } = &frame
            {
                self.refuse_first_snapshot = false;
                // Make the actual SQLite member's O_EXCL image creation fail.
                // Its normal refusal cleanup removes this owned-name remnant;
                // the proxy forwards the real measured refusal and Abort reply.
                let name = format!("capture-{}.sqlite.part", capture_id.0);
                let mut file = std::fs::OpenOptions::new()
                    .write(true)
                    .create_new(true)
                    .open(self.process.room.join(&name))
                    .unwrap();
                file.write_all(b"controlled image-name collision").unwrap();
                std::fs::write(
                    self.process
                        .root
                        .parent()
                        .unwrap()
                        .join("capture-collision.json"),
                    serde_json::to_vec(
                        &serde_json::json!({"capture_id":capture_id,"room_name":name}),
                    )
                    .unwrap(),
                )
                .unwrap();
            }
            if let s::StateFrame::Control {
                frame: s::StateControl::Abort { capture_id },
                ..
            } = &frame
                && let Some(late) = self.held_capture_result.take()
            {
                let response = s::decode_state_line(&late, s::FrameDirection::Response).unwrap();
                assert_eq!(response.common(), frame.common());
                assert!(matches!(response, s::StateFrame::ControlAnswer {
                    frame: s::StateControlAnswer::Snapshot { capture_id: actual, .. }
                        | s::StateControlAnswer::Finished { capture_id: actual, .. }, ..
                } if actual == *capture_id));
                // Forward the actual late result before the real member's
                // actual aborted reply, without creating either response here.
                self.to_harness.extend(late);
                self.capture_delay = None;
            }
            requests.push(label(&frame));
            if matches!(
                frame,
                s::StateFrame::File { .. } | s::StateFrame::FilePair { .. }
            ) {
                filings.push(frame);
            }
        }
        let joined = self
            .process
            .root
            .parent()
            .unwrap()
            .join("capture-join-retained");
        let collision = self
            .process
            .root
            .parent()
            .unwrap()
            .join("capture-collision.json");
        if !self.wait_for_capture_join || !collision.exists() || joined.exists() {
            flush(&mut self.stream, &mut self.to_member);
        }
        if self.harness_eof && self.to_member.is_empty() && !self.write_closed {
            self.stream.shutdown(std::net::Shutdown::Write).unwrap();
            self.write_closed = true;
        }
        if !self.member_eof {
            let mut bytes = [0; 65536];
            match self.stream.read(&mut bytes) {
                Ok(0) => self.member_eof = true,
                Ok(n) => {
                    self.response_bytes.extend_from_slice(&bytes[..n]);
                    self.response_lines.extend_from_slice(&bytes[..n]);
                }
                Err(e) if e.kind() == io::ErrorKind::WouldBlock => (),
                Err(e) => panic!("normal member response proxy: {e}"),
            }
        }
        while let Some(end) = self.response_lines.iter().position(|b| *b == b'\n') {
            let line: Vec<_> = self.response_lines.drain(..=end).collect();
            let mut response = s::decode_state_line(&line, s::FrameDirection::Response).unwrap();
            if self.damage_first_image
                && let s::StateFrame::ControlAnswer {
                    frame:
                        s::StateControlAnswer::Snapshot {
                            room_name,
                            size_bytes,
                            ..
                        },
                    ..
                } = &response
            {
                self.damage_first_image = false;
                // The member reports its real image unchanged. Damage that
                // owned room file before admin opens it, so the actual bounded
                // publisher refuses the size mismatch and sends Aborted.
                std::fs::OpenOptions::new()
                    .write(true)
                    .open(self.process.room.join(room_name))
                    .unwrap()
                    .set_len(size_bytes.0 + 1)
                    .unwrap();
                std::fs::write(
                    self.process
                        .root
                        .parent()
                        .unwrap()
                        .join("capture-preparation-failure.json"),
                    &line,
                )
                .unwrap();
            }
            if matches!(
                (&response, self.capture_delay),
                (
                    s::StateFrame::ControlAnswer {
                        frame: s::StateControlAnswer::Snapshot { .. },
                        ..
                    },
                    Some(CaptureDelay::Snapshot)
                ) | (
                    s::StateFrame::ControlAnswer {
                        frame: s::StateControlAnswer::Finished { .. },
                        ..
                    },
                    Some(CaptureDelay::Finished)
                )
            ) {
                assert!(self.held_capture_result.is_none());
                std::fs::write(
                    self.process
                        .root
                        .parent()
                        .unwrap()
                        .join("demand-late-result.jsonl"),
                    &line,
                )
                .unwrap();
                self.held_capture_result = Some(line);
                continue;
            }
            if matches!(
                response,
                s::StateFrame::ControlAnswer {
                    frame: s::StateControlAnswer::Aborted { .. },
                    ..
                }
            ) {
                assert!(self.held_capture_result.is_none());
                assert_eq!(std::fs::read_dir(&self.process.room).unwrap().count(), 0);
                std::fs::write(
                    self.process
                        .root
                        .parent()
                        .unwrap()
                        .join("demand-aborted.jsonl"),
                    &line,
                )
                .unwrap();
            }
            if matches!(
                response,
                s::StateFrame::ControlAnswer {
                    frame: s::StateControlAnswer::CaptureReleased { .. },
                    ..
                }
            ) {
                // Observe before forwarding the receipt. EOF cleanup can erase
                // a missed release-time deletion and cannot prove this boundary.
                let names: Vec<_> = std::fs::read_dir(&self.process.room)
                    .unwrap()
                    .map(|entry| entry.unwrap().file_name().to_string_lossy().into_owned())
                    .collect();
                std::fs::write(
                    self.process
                        .root
                        .parent()
                        .unwrap()
                        .join("release-room-files.json"),
                    serde_json::to_vec(&names).unwrap(),
                )
                .unwrap();
                assert!(
                    names.is_empty(),
                    "actual CaptureReleased retained room files: {names:?}"
                );
            }
            let line = if self.corrupt_abort
                && let s::StateFrame::ControlAnswer {
                    frame: s::StateControlAnswer::Aborted { capture_id },
                    ..
                } = &mut response
            {
                self.corrupt_abort = false;
                capture_id.0 += 1;
                let damaged = s::encode_state_line(&response, s::FrameDirection::Response).unwrap();
                std::fs::write(
                    self.process
                        .root
                        .parent()
                        .unwrap()
                        .join("demand-bad-aborted.jsonl"),
                    &damaged,
                )
                .unwrap();
                damaged
            } else {
                line
            };
            let line = if self.corrupt_release
                && let s::StateFrame::ControlAnswer {
                    frame: s::StateControlAnswer::CaptureReleased { capture_id },
                    ..
                } = &mut response
            {
                self.corrupt_release = false;
                capture_id.0 += 1;
                let damaged = s::encode_state_line(&response, s::FrameDirection::Response).unwrap();
                std::fs::write(
                    self.process
                        .root
                        .parent()
                        .unwrap()
                        .join("damaged-release.json"),
                    &damaged,
                )
                .unwrap();
                damaged
            } else {
                line
            };
            if self.treatment.is_some()
                && !self.intercepted
                && matches!(response, s::StateFrame::Filed { .. })
            {
                assert!(self.held_filing.is_none());
                self.intercepted = true;
                let root = self.process.root.parent().unwrap();
                std::fs::write(root.join("original-filing.jsonl"), &line).unwrap();
                if self.treatment == Some(ReceiptTreatment::Fragment) {
                    self.held_prefix = line.len() / 2;
                    self.to_harness.extend_from_slice(&line[..self.held_prefix]);
                    std::fs::write(root.join("filing-prefix.bin"), &line[..self.held_prefix])
                        .unwrap();
                }
                self.held_filing = Some(line);
            } else {
                self.to_harness.extend(line);
            }
        }
        flush(peer, &mut self.to_harness);
        if self.member_eof && self.to_harness.is_empty() && !self.output_closed {
            peer.shutdown(std::net::Shutdown::Write).unwrap();
            self.output_closed = true;
        }
    }

    pub(super) fn filing_held(&self) -> bool {
        self.held_filing.is_some()
    }

    pub(super) fn assert_capture(&self, report: &s::SavePointReport, filings: &[s::StateFrame]) {
        let root = self.process.root.parent().unwrap();
        let image = std::fs::read(root.join("retained-capture.sqlite")).unwrap();
        verify_image(
            root,
            &image,
            report,
            &self.request_bytes,
            filings,
            filings
                .iter()
                .filter(|frame| {
                    matches!(frame,
                        s::StateFrame::File { filing, .. } if filing.source.kind == "message.user"
                    )
                })
                .count(),
        );
    }

    pub(super) fn finish(
        &mut self,
        peer: &mut UnixStream,
        requests: &mut Vec<String>,
        filings: &mut Vec<s::StateFrame>,
    ) {
        let end = Instant::now() + Duration::from_secs(5);
        loop {
            self.step(peer, requests, filings);
            if self.process.exited() {
                break;
            }
            assert!(Instant::now() < end, "worker left the normal member alive");
            std::thread::sleep(Duration::from_millis(1));
        }
        assert!(self.harness_eof && self.write_closed);
        assert!(self.to_member.is_empty() && self.to_harness.is_empty());
        self.write_evidence(filings);
    }

    pub(super) fn write_evidence(&self, filings: &[s::StateFrame]) {
        let root = self.process.root.parent().unwrap();
        std::fs::write(root.join("member-requests.jsonl"), &self.request_bytes).unwrap();
        std::fs::write(root.join("member-responses.jsonl"), &self.response_bytes).unwrap();
        std::fs::write(
            root.join("member-filings.json"),
            serde_json::to_vec(filings).unwrap(),
        )
        .unwrap();
        if let Some(path) = std::env::var_os("WEAVER_NORMAL_MEMBER_EVIDENCE") {
            let path = PathBuf::from(path);
            std::fs::create_dir_all(&path).unwrap();
            std::fs::write(path.join("requests.jsonl"), &self.request_bytes).unwrap();
            std::fs::write(path.join("responses.jsonl"), &self.response_bytes).unwrap();
            std::fs::write(path.join("seed.sqlite"), &self.image).unwrap();
            if let Some(bytes) = &self.released_filing {
                std::fs::write(path.join("held-filing.json"), bytes).unwrap();
            }
        }
    }
}

pub(super) fn verify_image(
    root: &Path,
    image: &[u8],
    report: &s::SavePointReport,
    requests: &[u8],
    filings: &[s::StateFrame],
    expected_filings: usize,
) {
    assert_eq!(digest(image), report.start.dump_sha256);
    let mut open = s::decode_state_line(
        requests.split_inclusive(|b| *b == b'\n').next().unwrap(),
        s::FrameDirection::Request,
    )
    .unwrap();
    let s::StateFrame::Open {
        common,
        start,
        read_limits,
        ..
    } = &mut open
    else {
        panic!("original Open");
    };
    *start = report.start.clone();
    common.run_id = "normal-capture-reader".into();
    common.request_id = 1.into();
    let common = common.clone();
    let max = read_limits.max_result_bytes;
    let (mut process, stream) = Process::launch(&root.join("normal-capture-reader"), Some(image));
    let mut inspector = Inspector {
        channel: BufReader::new(stream),
        common,
        raw: vec![],
        max,
    };
    assert!(matches!(
        inspector.exchange(open),
        s::StateFrame::Opened { .. }
    ));
    assert_eq!(
        inspector.control(s::StateControl::Position {}),
        s::StateControlAnswer::Position(report.start.position.clone())
    );
    let s::QueryResult::Entries { entries } = inspector.query(s::ReadSelector::SystemPrompt {})
    else {
        panic!("prompt entries");
    };
    assert_eq!(entries.len(), 1);
    assert_eq!(entries[0].text, "Read item.");
    assert!(
        matches!(&entries[0].source, s::ReadSource::Seed(seed) if seed.seed_id == report.start.seed_id)
    );
    let mut found = 0;
    for frame in filings {
        let s::StateFrame::File { filing, .. } = frame else {
            continue;
        };
        let captured = report.start.position.as_ref().is_some_and(|position| {
            filing.block_id.run == position.block_id.run && filing.block_id <= position.block_id
        });
        found += usize::from(captured && filing.source.kind == "message.user");
        let noun = filing
            .bindings
            .iter()
            .flat_map(|binding| &binding.nouns)
            .next()
            .expect("this capture fixture files statements with nouns");
        let s::QueryResult::Entries { entries } = inspector.query(s::ReadSelector::History {
            noun: noun.value.clone(),
            verb: None,
            condition: None,
        }) else {
            panic!("history entries");
        };
        let source = s::ReadSource::Event(Box::new(s::EventSource {
            block_id: filing.block_id.clone(),
            envelope: filing.source.clone(),
            source_path: filing.source_path.clone(),
            partition_rule_id: filing.partition_rule_id.clone(),
        }));
        let matches: Vec<_> = entries
            .iter()
            .filter(|entry| entry.source == source)
            .collect();
        assert_eq!(
            matches.len(),
            usize::from(captured),
            "the immutable image contains exactly the filings through its source position"
        );
        if !captured {
            continue;
        }
        let entry = matches[0];
        assert_eq!(entry.text, filing.text);
        assert_eq!(entry.raw_block.get(), filing.raw_block.get());
        assert_eq!(
            entry.source,
            s::ReadSource::Event(Box::new(s::EventSource {
                block_id: filing.block_id.clone(),
                envelope: filing.source.clone(),
                source_path: filing.source_path.clone(),
                partition_rule_id: filing.partition_rule_id.clone(),
            }))
        );
        assert_eq!(
            entry
                .bindings
                .iter()
                .map(|b| b.value.clone())
                .collect::<Vec<_>>(),
            filing.bindings
        );
        assert!(
            entry
                .bindings
                .iter()
                .enumerate()
                .all(|(i, binding)| binding.ordinal.0 == i as u64)
        );
        verify_pairs(&entry.pairs, &filing.block_id, filings);
    }
    assert_eq!(found, expected_filings);
    if let Some(path) = std::env::var_os("WEAVER_NORMAL_MEMBER_EVIDENCE") {
        std::fs::write(
            Path::new(&path).join("capture-reader-responses.jsonl"),
            &inspector.raw,
        )
        .unwrap();
    }
    std::fs::write(root.join("capture-reader-responses.jsonl"), &inspector.raw).unwrap();
    drop(inspector);
    process.wait();
}

fn verify_pairs(pairs: &[s::ReadPair], block: &s::SourceBlockId, filings: &[s::StateFrame]) {
    let openings: Vec<_> = filings
        .iter()
        .filter_map(|frame| match frame {
            s::StateFrame::FilePair { filing, .. } => match &filing.pair {
                s::PairRecord::Started(start) if start.statement.block_id == *block => {
                    Some((filing, start))
                }
                _ => None,
            },
            _ => None,
        })
        .collect();
    assert_eq!(
        pairs.len(),
        openings.len(),
        "every original pair survives capture exactly once"
    );
    let source = |filing: &s::PairFile| s::EventSource {
        block_id: filing.block_id.clone(),
        envelope: filing.source.clone(),
        source_path: filing.source_path.clone(),
        partition_rule_id: filing.partition_rule_id.clone(),
    };
    for (opening, start) in openings {
        let matches: Vec<_> = pairs
            .iter()
            .filter(|pair| pair.call_id == start.call_id)
            .collect();
        assert_eq!(matches.len(), 1);
        let pair = matches[0];
        assert_eq!(pair.statement, start.statement);
        assert_eq!(pair.lane, start.lane);
        assert_eq!(pair.request, start.request);
        assert_eq!(pair.opening_source.source, source(opening));
        assert_eq!(pair.opening_source.raw_block.get(), opening.raw_block.get());
        let endings: Vec<_> = filings
            .iter()
            .filter_map(|frame| match frame {
                s::StateFrame::FilePair { filing, .. } => match &filing.pair {
                    s::PairRecord::Ended(end) if end.call_id == start.call_id => {
                        Some((filing, end))
                    }
                    _ => None,
                },
                _ => None,
            })
            .collect();
        assert!(endings.len() <= 1);
        if let Some((terminal, end)) = endings.first() {
            assert_eq!(
                pair.fate,
                match end.fate {
                    s::TerminalPairFate::Completed => s::PairFate::Completed,
                    s::TerminalPairFate::Failed => s::PairFate::Failed,
                }
            );
            assert_eq!(pair.reason, end.reason);
            let actual = pair.terminal_source.as_ref().unwrap();
            assert_eq!(actual.source, source(terminal));
            assert_eq!(actual.raw_block.get(), terminal.raw_block.get());
        } else {
            assert_eq!(pair.fate, s::PairFate::Opened);
            assert_eq!(pair.reason, None);
            assert!(pair.terminal_source.is_none());
        }
    }
}

struct Inspector {
    channel: BufReader<UnixStream>,
    common: s::Common,
    raw: Vec<u8>,
    max: s::DecimalU64,
}
impl Inspector {
    fn exchange(&mut self, request: s::StateFrame) -> s::StateFrame {
        self.channel
            .get_mut()
            .write_all(&s::encode_state_line(&request, s::FrameDirection::Request).unwrap())
            .unwrap();
        let mut line = vec![];
        self.channel.read_until(b'\n', &mut line).unwrap();
        self.raw.extend_from_slice(&line);
        let answer = s::decode_state_line(&line, s::FrameDirection::Response).unwrap();
        assert_eq!(answer.common(), request.common());
        self.common.request_id = (self.common.request_id.0 + 1).into();
        answer
    }
    fn control(&mut self, frame: s::StateControl) -> s::StateControlAnswer {
        let response = self.exchange(s::StateFrame::Control {
            common: self.common.clone(),
            frame,
        });
        let s::StateFrame::ControlAnswer { frame, .. } = response else {
            panic!("control refused: {response:?}");
        };
        frame
    }
    fn query(&mut self, selector: s::ReadSelector) -> s::QueryResult {
        let answer = self.control(s::StateControl::Read {
            selector: selector.clone(),
        });
        let s::StateControlAnswer::ReadPage(mut page) = answer else {
            panic!("query refused: {answer:?}");
        };
        let id = page.snapshot_id;
        let total = page.total_bytes;
        let mut bytes = vec![];
        loop {
            assert_eq!(page.offset.0, bytes.len() as u64);
            assert_eq!(page.snapshot_id, id);
            assert_eq!(page.total_bytes, total);
            bytes.extend(page.decode_octets(self.max).unwrap());
            if page.done {
                break;
            }
            let answer = self.control(s::StateControl::Next {
                snapshot_id: id,
                offset: (bytes.len() as u64).into(),
            });
            let s::StateControlAnswer::ReadPage(next) = answer else {
                panic!("page refused: {answer:?}");
            };
            page = next;
        }
        assert_eq!(bytes.len() as u64, total.0);
        assert_eq!(
            self.control(s::StateControl::Release { snapshot_id: id }),
            s::StateControlAnswer::Released { snapshot_id: id }
        );
        let result: s::QueryResult =
            s::decode_strict(std::str::from_utf8(&bytes).unwrap()).unwrap();
        assert!(result.matches_selector(&selector));
        result
    }
}

fn flush(stream: &mut UnixStream, bytes: &mut Vec<u8>) {
    if bytes.is_empty() {
        return;
    }
    match stream.write(bytes) {
        Ok(0) => panic!("proxy made a zero-byte write"),
        Ok(n) => {
            bytes.drain(..n);
        }
        Err(e) if e.kind() == io::ErrorKind::WouldBlock => (),
        Err(e) => panic!("normal member proxy write: {e}"),
    }
}

fn label(frame: &s::StateFrame) -> String {
    match frame {
        s::StateFrame::Open { .. } => "open",
        s::StateFrame::File { .. } => "file",
        s::StateFrame::FilePair { .. } => "pair-file",
        s::StateFrame::Control { frame, .. } => match frame {
            s::StateControl::Grants {} => "grants",
            s::StateControl::Read { selector } => match selector {
                s::ReadSelector::SystemPrompt {} => "prompt",
                s::ReadSelector::OpenPairs {} => "pairs",
                s::ReadSelector::PlanCurrent {} => "summary",
                s::ReadSelector::Marks {} => "marks",
                other => return format!("read:{other:?}"),
            },
            s::StateControl::Release { .. } => "release",
            s::StateControl::Snapshot { .. } => "snapshot",
            s::StateControl::Acknowledge { .. } => "acknowledge",
            s::StateControl::ReleaseCapture { .. } => "release_capture",
            s::StateControl::Abort { .. } => "abort",
            other => return format!("control:{other:?}"),
        },
        other => panic!("not a state request: {other:?}"),
    }
    .into()
}
