//! The normal executable, with the descriptors admin supplies across exec.

use std::fs::{File, OpenOptions};
use std::io::{BufRead, BufReader, Write};
use std::os::fd::{AsRawFd, FromRawFd, OwnedFd, RawFd};
use std::os::unix::net::UnixStream;
use std::os::unix::process::CommandExt;
use std::path::PathBuf;
use std::process::{Child, Command, Stdio};
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{Duration, Instant};

use weaver_state::engine::serving_sqlite::seed_image;
use weaver_types::state_flow::*;

use super::fixtures;

static NEXT_DIRECTORY: AtomicU64 = AtomicU64::new(1);

pub struct Scratch(pub PathBuf);

impl Scratch {
    fn new() -> Self {
        let name = format!(
            "weaver-serving-member-{}-{}",
            std::process::id(),
            NEXT_DIRECTORY.fetch_add(1, Ordering::Relaxed)
        );
        let path = std::env::temp_dir().join(name);
        std::fs::create_dir(&path).unwrap();
        std::fs::create_dir(path.join("room")).unwrap();
        Self(path)
    }
}

impl Drop for Scratch {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

pub enum ImageDescriptor {
    Absent,
    ReadOnly,
    Writable,
    Socket,
}

pub struct Member {
    child: Child,
    pub channel: BufReader<UnixStream>,
    scratch: Scratch,
    pub seed: SeedRequest,
    pub start: StartIdentity,
    pub next_request: u64,
    next_operation: u64,
    pub observations: Vec<StoredOperationObservation>,
    pub max_result_bytes: DecimalU64,
    pub identity: Option<MemberObservationIdentity>,
}

fn high_copy(fd: RawFd) -> OwnedFd {
    // SAFETY: the test owns fd, and F_DUPFD_CLOEXEC returns a fresh descriptor.
    let copied = unsafe { nix::libc::fcntl(fd, nix::libc::F_DUPFD_CLOEXEC, 100) };
    assert!(copied >= 100);
    // SAFETY: this is the newly allocated descriptor, with exactly one owner.
    unsafe { OwnedFd::from_raw_fd(copied) }
}

impl Member {
    pub fn launch(
        mode: &str,
        image_descriptor: ImageDescriptor,
        wrong_door: bool,
        preload: bool,
    ) -> Self {
        let seed = fixtures::seed();
        let image = seed_image(&seed).unwrap().into_bytes();
        Self::launch_image(mode, image_descriptor, wrong_door, preload, seed, &image)
    }

    pub fn launch_image(
        mode: &str,
        image_descriptor: ImageDescriptor,
        wrong_door: bool,
        preload: bool,
        seed: SeedRequest,
        image: &[u8],
    ) -> Self {
        let scratch = Scratch::new();
        let image_path = scratch.0.join("input.sqlite");
        std::fs::write(&image_path, image).unwrap();
        let start = fixtures::start(image, &seed);
        let (ours, theirs) = UnixStream::pair().unwrap();
        ours.set_read_timeout(Some(Duration::from_secs(10)))
            .unwrap();
        ours.set_write_timeout(Some(Duration::from_secs(10)))
            .unwrap();
        let door = if wrong_door {
            high_copy(File::open(&image_path).unwrap().as_raw_fd())
        } else {
            high_copy(theirs.as_raw_fd())
        };
        drop(theirs);
        let image_fd = match image_descriptor {
            ImageDescriptor::Absent => None,
            ImageDescriptor::ReadOnly => {
                Some(high_copy(File::open(&image_path).unwrap().as_raw_fd()))
            }
            ImageDescriptor::Writable => Some(high_copy(
                OpenOptions::new()
                    .read(true)
                    .write(true)
                    .open(&image_path)
                    .unwrap()
                    .as_raw_fd(),
            )),
            ImageDescriptor::Socket => Some(high_copy(ours.as_raw_fd())),
        };
        let lock = File::create(scratch.0.join("run.lock")).unwrap();
        // SAFETY: flock acts on a file owned by this fixture and is inherited by exec.
        assert_eq!(
            unsafe { nix::libc::flock(lock.as_raw_fd(), nix::libc::LOCK_EX) },
            0
        );
        let lock = high_copy(lock.as_raw_fd());
        let door_fd = door.as_raw_fd();
        let lock_fd = lock.as_raw_fd();
        let image_fd = image_fd.as_ref().map(AsRawFd::as_raw_fd);
        let mut command = Command::new(env!("CARGO_BIN_EXE_weaver-state"));
        command
            .arg(format!("--mode={mode}"))
            .arg(scratch.0.join("room"));
        if preload {
            command.arg(scratch.0.join("preload.sock"));
        }
        command
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(File::create(scratch.0.join("stderr")).unwrap());
        // SAFETY: only async-signal-safe descriptor operations occur between fork/exec.
        // Sources are above 99, so installing a destination cannot destroy a source.
        unsafe {
            command.pre_exec(move || {
                if nix::libc::dup2(door_fd, 3) < 0 || nix::libc::dup2(lock_fd, 9) < 0 {
                    return Err(std::io::Error::last_os_error());
                }
                if let Some(image_fd) = image_fd {
                    if nix::libc::dup2(image_fd, 4) < 0 {
                        return Err(std::io::Error::last_os_error());
                    }
                } else {
                    nix::libc::close(4);
                }
                Ok(())
            });
        }
        let child = command
            .spawn()
            .expect("build the normal weaver-state binary before this test");
        Self {
            child,
            channel: BufReader::new(ours),
            scratch,
            seed,
            start,
            next_request: 1,
            next_operation: 1,
            observations: vec![],
            max_result_bytes: 4_000_000.into(),
            identity: None,
        }
    }

    pub fn serving() -> Self {
        let mut member = Self::launch("serving", ImageDescriptor::ReadOnly, false, false);
        member.open();
        member
    }

    pub fn open_request(&mut self) -> StateFrame {
        StateFrame::Open {
            common: self.common(),
            schema_id: self.seed.schema_id.clone(),
            scheme_id: self.seed.scheme_id.clone(),
            start: self.start.clone(),
            read_limits: fixtures::limits(self.max_result_bytes.0),
        }
    }

    pub fn open(&mut self) {
        let request = self.open_request();
        let response = self.exchange(&request);
        let StateFrame::Opened {
            member_observation, ..
        } = response
        else {
            panic!("open failed: {response:?}, {}", self.log());
        };
        assert_eq!(member_observation.pid, self.child.id());
        self.identity = Some(member_observation);
    }

    pub fn common(&mut self) -> Common {
        let id = self.next_request;
        self.next_request += 1;
        fixtures::common(id)
    }

    pub fn send(&mut self, request: &StateFrame) {
        let bytes = encode_state_line(request, FrameDirection::Request).unwrap();
        self.channel.get_mut().write_all(&bytes).unwrap();
    }

    pub fn response(&mut self, request: &StateFrame) -> StateFrame {
        let bytes = self.line();
        let response =
            decode_state_line(&bytes, FrameDirection::Response).unwrap_or_else(|error| {
                panic!(
                    "invalid state answer: {error}, {}, bytes {bytes:?}",
                    self.log()
                );
            });
        assert_eq!(
            bytes,
            encode_state_line(&response, FrameDirection::Response).unwrap(),
            "the actual reply must preserve the shared contract's canonical field order"
        );
        response
            .validate_response_to(request, self.next_operation.into())
            .unwrap();
        let observation = response.observation().unwrap().clone();
        observation.validate_shape().unwrap();
        if observation.operation != StateOperation::Snapshot {
            assert_eq!(observation.resource.cpu_ns, None);
            assert_eq!(observation.disk.enabled, None);
            assert_eq!(observation.disk.before, None);
            assert_eq!(observation.disk.after, None);
            assert_eq!(
                observation.disk.unavailable_reason,
                Some(ObservationUnavailable::NotSampled)
            );
        }
        self.observations.push(StoredOperationObservation {
            request_source: match request {
                StateFrame::File { filing, .. } => Some(filing.block_id.clone()),
                StateFrame::FilePair { filing, .. } => Some(filing.block_id.clone()),
                _ => None,
            },
            observation,
        });
        self.next_operation += 1;
        response
    }

    pub fn exchange(&mut self, request: &StateFrame) -> StateFrame {
        self.send(request);
        self.response(request)
    }

    pub fn control(&mut self, frame: StateControl) -> StateControlAnswer {
        let common = self.common();
        self.control_at(common, frame)
    }

    pub fn control_at(&mut self, common: Common, frame: StateControl) -> StateControlAnswer {
        let request = StateFrame::Control { common, frame };
        let response = self.exchange(&request);
        match response {
            StateFrame::ControlAnswer { frame, .. } => frame,
            other => panic!("expected control answer, got {other:?}"),
        }
    }

    pub fn file(&mut self, filing: StatementFile) -> StateFrame {
        let request = StateFrame::File {
            common: self.common(),
            filing,
        };
        self.exchange(&request)
    }

    pub fn file_pair(&mut self, filing: PairFile) -> StateFrame {
        let request = StateFrame::FilePair {
            common: self.common(),
            filing,
        };
        self.exchange(&request)
    }

    pub fn query(&mut self, selector: ReadSelector) -> QueryResult {
        let answer = self.control(StateControl::Read {
            selector: selector.clone(),
        });
        let StateControlAnswer::ReadPage(mut page) = answer else {
            panic!("expected read page: {answer:?}");
        };
        let snapshot_id = page.snapshot_id;
        let total = page.total_bytes;
        let mut bytes = Vec::new();
        loop {
            assert_eq!(page.offset.0, bytes.len() as u64);
            assert_eq!(page.total_bytes, total);
            assert_eq!(page.snapshot_id, snapshot_id);
            bytes.extend(page.decode_octets(self.max_result_bytes).unwrap());
            if page.done {
                break;
            }
            let answer = self.control(StateControl::Next {
                snapshot_id,
                offset: (bytes.len() as u64).into(),
            });
            let StateControlAnswer::ReadPage(next) = answer else {
                panic!("{answer:?}");
            };
            page = next;
        }
        assert_eq!(bytes.len() as u64, total.0);
        assert_eq!(
            self.control(StateControl::Release { snapshot_id }),
            StateControlAnswer::Released { snapshot_id }
        );
        let result: QueryResult = decode_strict(std::str::from_utf8(&bytes).unwrap()).unwrap();
        assert!(result.matches_selector(&selector));
        result
    }

    pub fn line(&mut self) -> Vec<u8> {
        let mut bytes = Vec::new();
        self.channel
            .read_until(b'\n', &mut bytes)
            .unwrap_or_else(|error| {
                panic!("member response failed: {error}, {}", self.log());
            });
        assert!(!bytes.is_empty(), "member closed: {}", self.log());
        assert!(bytes.len() <= STATE_ANSWER_LINE_BOUND);
        bytes
    }

    pub fn assert_closed(&mut self) {
        let mut bytes = Vec::new();
        let result = self.channel.read_until(b'\n', &mut bytes);
        assert!(
            matches!(result, Ok(0))
                || result
                    .as_ref()
                    .is_err_and(|error| error.kind() == std::io::ErrorKind::ConnectionReset),
            "expected closed channel, received {result:?}, {bytes:?}, {}",
            self.log()
        );
        self.wait();
    }

    pub fn shutdown(&mut self) {
        self.channel
            .get_mut()
            .shutdown(std::net::Shutdown::Both)
            .unwrap();
        self.wait();
    }

    fn wait(&mut self) {
        let deadline = Instant::now() + Duration::from_secs(5);
        while self.child.try_wait().unwrap().is_none() {
            assert!(
                Instant::now() < deadline,
                "member did not exit: {}",
                self.log()
            );
            std::thread::sleep(Duration::from_millis(5));
        }
    }

    pub fn room(&self) -> PathBuf {
        self.scratch.0.join("room")
    }
    pub fn log(&self) -> String {
        std::fs::read_to_string(self.scratch.0.join("stderr")).unwrap_or_default()
    }
    pub fn pid(&self) -> u32 {
        self.child.id()
    }
    pub fn room_names(&self) -> Vec<String> {
        let mut names: Vec<_> = std::fs::read_dir(self.room())
            .unwrap()
            .map(|entry| entry.unwrap().file_name().into_string().unwrap())
            .collect();
        names.sort();
        names
    }
}

impl Drop for Member {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}
