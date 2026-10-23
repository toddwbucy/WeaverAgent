//! The built STE100 duty owns one inherited seam and cannot select a test adapter.
use super::*;
use nix::libc;
use nix::sys::socket::{AddressFamily, SockFlag, SockType, socketpair};
use weaver_spu::ste100::channel::{Channel, Message, Receive, SendProgress, Tag};
use weaver_types::state_flow::{Ste100AdmissionCode, Ste100AdmissionRefused, decode_strict};

#[path = "../common/ste100_fixture.rs"]
mod fixture;

struct Process {
    child: Child,
    log: PathBuf,
}
impl Process {
    fn spawn(args: &[&str], ends: &[RawFd]) -> Self {
        let log = std::env::temp_dir().join(format!(
            "weaver-ste100-entry-{}-{}.log",
            std::process::id(),
            COUNTER.fetch_add(1, Ordering::Relaxed)
        ));
        let mut command = Command::new(env!("CARGO_BIN_EXE_weaver-spu-ste100"));
        command
            .args(args)
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::from(File::create(&log).unwrap()));
        place_inherited(&mut command, ends);
        Self {
            child: command.spawn().expect("built STE100 duty starts"),
            log,
        }
    }
    fn wait(&mut self) -> std::process::ExitStatus {
        wait_bounded(&mut self.child, 30, "STE100 entry")
    }
}
impl Drop for Process {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
        let _ = std::fs::remove_file(&self.log);
    }
}
fn writable(peer: &Channel, events: i16) {
    let mut pollfd = libc::pollfd {
        fd: peer.readiness().as_raw_fd(),
        events,
        revents: 0,
    };
    let result = unsafe { libc::poll(&mut pollfd, 1, 30_000) };
    assert!(
        result > 0,
        "bounded STE100 child exchange stalled: {}",
        std::io::Error::last_os_error()
    );
}
fn send(peer: &mut Channel, bytes: Vec<u8>) {
    peer.begin_send(Tag::Bootstrap, 0, bytes).unwrap();
    while peer.has_pending_send() {
        if peer.try_send().unwrap() == SendProgress::Pending {
            writable(peer, libc::POLLOUT);
        }
    }
}
fn receive(peer: &mut Channel) -> Message {
    loop {
        match peer.try_receive().unwrap() {
            Receive::Complete(message) => return message,
            Receive::Fragment => {}
            Receive::Pending => writable(peer, libc::POLLIN),
        }
    }
}

#[test]
fn ste100_entry_requires_the_exact_positive_decimal_launch_vector() {
    for args in [
        vec![],
        vec!["--max-wire-bytes"],
        vec!["--other", "65536"],
        vec!["--max-wire-bytes", "0"],
        vec!["--max-wire-bytes", "-1"],
        vec!["--max-wire-bytes", "+1"],
        vec!["--max-wire-bytes", "01"],
        vec!["--max-wire-bytes", "1.0"],
        vec!["--max-wire-bytes", " 1"],
        vec!["--max-wire-bytes", "18446744073709551616"],
        vec!["--max-wire-bytes", "65536", "extra"],
        vec!["--max-wire-bytes", "65536", "--max-wire-bytes", "65536"],
    ] {
        let (peer, child) = seqpacket_pair();
        let mut process = Process::spawn(&args, &[child.as_raw_fd()]);
        drop((peer, child));
        assert!(
            !process.wait().success(),
            "invalid launch vector accepted: {args:?}; {}",
            complaint(&process.log)
        );
        assert!(
            complaint(&process.log).contains("STE100 launch:"),
            "launch validation must itself refuse {args:?}: {}",
            complaint(&process.log)
        );
    }
}

#[test]
fn ste100_entry_refuses_missing_leaked_and_wrong_kind_descriptors() {
    let mut missing = Process::spawn(&["--max-wire-bytes", "65536"], &[]);
    assert!(!missing.wait().success(), "missing fd3 was admitted");
    assert!(
        complaint(&missing.log).contains("DescriptorCountWrong { found: 0 }"),
        "{}",
        complaint(&missing.log)
    );
    let (peer, child) = seqpacket_pair();
    let (leaked_peer, leaked_child) = seqpacket_pair();
    let mut leaked = Process::spawn(
        &["--max-wire-bytes", "65536"],
        &[child.as_raw_fd(), leaked_child.as_raw_fd()],
    );
    drop((peer, child, leaked_peer, leaked_child));
    assert!(
        !leaked.wait().success(),
        "second inherited handle was admitted"
    );
    assert!(
        complaint(&leaked.log).contains("DescriptorCountWrong { found: 2 }"),
        "{}",
        complaint(&leaked.log)
    );
    for kind in [SockType::Stream, SockType::Datagram] {
        let (peer, child) =
            socketpair(AddressFamily::Unix, kind, None, SockFlag::SOCK_CLOEXEC).unwrap();
        let mut process = Process::spawn(&["--max-wire-bytes", "65536"], &[child.as_raw_fd()]);
        drop((peer, child));
        assert!(
            !process.wait().success(),
            "non-SEQPACKET inherited end was admitted: {kind:?}"
        );
        assert!(
            complaint(&process.log).contains("WrongSocket"),
            "socket validation must itself refuse {kind:?}: {}",
            complaint(&process.log)
        );
    }
}

#[test]
fn ste100_entry_healthy_single_seam_reaches_the_actual_bootstrap_validator() {
    let (peer, child) = seqpacket_pair();
    let mut process = Process::spawn(&["--max-wire-bytes", "65536"], &[child.as_raw_fd()]);
    drop(child);
    let mut peer = Channel::from_owned(peer, fixture::LIMIT).unwrap();
    send(&mut peer, b"{}".to_vec());
    let message = receive(&mut peer);
    assert_eq!(message.tag, Tag::AdmissionRefused);
    let refusal: Ste100AdmissionRefused =
        decode_strict(std::str::from_utf8(&message.payload).unwrap()).unwrap();
    assert_eq!(refusal.code, Ste100AdmissionCode::InvalidInstruction);
    process.wait();
}

#[test]
fn ste100_entry_production_refuses_both_fixture_and_uncompiled_candidate_without_ready() {
    for fixture_marker in [false, true] {
        let fixture = fixture::Fixture::new(fixture_marker);
        let (peer, child) = seqpacket_pair();
        let mut process = Process::spawn(&["--max-wire-bytes", "65536"], &[child.as_raw_fd()]);
        drop(child);
        let mut peer = Channel::from_owned(peer, fixture::LIMIT).unwrap();
        send(&mut peer, serde_json::to_vec(&fixture.bootstrap).unwrap());
        let message = receive(&mut peer);
        assert_eq!(message.tag, Tag::AdmissionRefused);
        assert_eq!(message.id, 0);
        let refusal: Ste100AdmissionRefused =
            decode_strict(std::str::from_utf8(&message.payload).unwrap()).unwrap();
        assert_eq!(refusal.code, Ste100AdmissionCode::UnsupportedArtifact);
        if fixture_marker {
            assert!(
                refusal.detail.contains("fixture-read/1"),
                "{}",
                refusal.detail
            );
        } else {
            assert_eq!(
                refusal.detail,
                "no compatible STE100 adapter is compiled into this executable"
            );
        }
        process.wait();
        assert!(
            peer.try_receive().is_err(),
            "terminal admission refusal must close rather than emit Ready"
        );
    }
}

#[test]
fn ste100_entry_validates_bootstrap_limit_before_declared_path_acquisition() {
    let mut fixture = fixture::Fixture::new(false);
    fixture.bootstrap.instruction.model_binding.artifact.0 = fixture
        .root
        .join("absent-manifest")
        .to_str()
        .unwrap()
        .into();
    let (peer, child) = seqpacket_pair();
    let mut process = Process::spawn(&["--max-wire-bytes", "65535"], &[child.as_raw_fd()]);
    drop(child);
    let mut peer = Channel::from_owned(peer, fixture::LIMIT).unwrap();
    send(&mut peer, serde_json::to_vec(&fixture.bootstrap).unwrap());
    let message = receive(&mut peer);
    assert_eq!(message.tag, Tag::AdmissionRefused);
    let refusal: Ste100AdmissionRefused =
        decode_strict(std::str::from_utf8(&message.payload).unwrap()).unwrap();
    assert_eq!(refusal.code, Ste100AdmissionCode::InvalidInstruction);
    process.wait();
}

#[test]
fn ste100_entry_clears_dumpability_before_waiting_for_bootstrap() {
    use std::os::unix::fs::MetadataExt;
    if nix::unistd::geteuid().is_root() {
        eprintln!(
            "SKIP ste100_entry_clears_dumpability_before_waiting_for_bootstrap: root owns /proc either way"
        );
        return;
    }
    let (peer, child) = seqpacket_pair();
    let mut process = Process::spawn(&["--max-wire-bytes", "65536"], &[child.as_raw_fd()]);
    drop(child);
    let deadline = Instant::now() + Duration::from_secs(30);
    let mut owner = None;
    while Instant::now() < deadline {
        if let Ok(meta) = std::fs::metadata(format!("/proc/{}/fd", process.child.id())) {
            owner = Some(meta.uid());
            if owner == Some(0) {
                break;
            }
        }
        assert!(
            process.child.try_wait().unwrap().is_none(),
            "duty exited before bootstrap: {}",
            complaint(&process.log)
        );
        std::thread::yield_now();
    }
    drop(peer);
    process.wait();
    assert_eq!(owner, Some(0), "kernel did not observe cleared dumpability");
}

#[test]
fn ste100_entry_adopts_fd_once_with_cloexec_and_real_close_in_isolated_child() {
    const CHILD: &str = "STE100_ENTRY_DESCRIPTOR_PROBE";
    const COMPLETE: &str = "STE100_OWNED_FD_CLOEXEC_DUMPABLE_CLOSED";
    if std::env::var_os(CHILD).is_some() {
        use std::os::fd::AsFd;
        let owned = weaver_spu::channel::adopt_ste100().unwrap();
        assert_eq!(owned.as_raw_fd(), 3);
        let flags = nix::fcntl::fcntl(owned.as_fd(), nix::fcntl::FcntlArg::F_GETFD).unwrap();
        assert_ne!(flags & libc::FD_CLOEXEC, 0);
        assert_eq!(unsafe { libc::prctl(libc::PR_GET_DUMPABLE) }, 0);
        assert_eq!(
            nix::sys::socket::getsockopt(&owned, nix::sys::socket::sockopt::SockType).unwrap(),
            SockType::SeqPacket
        );
        nix::sys::socket::getpeername::<nix::sys::socket::UnixAddr>(owned.as_raw_fd()).unwrap();
        let raw = owned.as_raw_fd();
        drop(owned);
        assert_eq!(unsafe { libc::fcntl(raw, libc::F_GETFD) }, -1);
        assert_eq!(
            std::io::Error::last_os_error().raw_os_error(),
            Some(libc::EBADF)
        );
        println!("\n{COMPLETE}");
        return;
    }
    let (_peer, child_end) = seqpacket_pair();
    let mut command = Command::new(std::env::current_exe().unwrap());
    command
        .args([
            "--exact",
            "ste100::ste100_entry_adopts_fd_once_with_cloexec_and_real_close_in_isolated_child",
            "--nocapture",
            "--test-threads=1",
        ])
        .env(CHILD, "1")
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    place_inherited(&mut command, &[child_end.as_raw_fd()]);
    let mut child = command.spawn().unwrap();
    let status = wait_bounded(&mut child, 30, "isolated STE100 inherited descriptor proof");
    use std::io::Read as _;
    let mut stdout = String::new();
    child
        .stdout
        .take()
        .unwrap()
        .read_to_string(&mut stdout)
        .unwrap();
    let mut stderr = String::new();
    child
        .stderr
        .take()
        .unwrap()
        .read_to_string(&mut stderr)
        .unwrap();
    assert!(
        status.success(),
        "descriptor child failed: {stdout}\n{stderr}"
    );
    assert_eq!(
        stdout.lines().filter(|line| *line == COMPLETE).count(),
        1,
        "exact child must execute all actual descriptor checks: {stdout}\n{stderr}"
    );
}
